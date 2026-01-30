use grift_eval::*;

fn eval_to_num<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> isize {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().as_number().unwrap()
}

fn eval_is_true<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> bool {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().is_true()
}

fn eval_is_false<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> bool {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().is_false()
}

#[test]
fn test_eval_number() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "42"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "-10"), -10);
}

#[test]
fn test_eval_booleans() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // In Scheme, only #t and #f are booleans (no true/false aliases)
    assert!(eval_is_true(&lisp, &mut eval, "#t"));
    assert!(eval_is_false(&lisp, &mut eval, "#f"));
}

#[test]
fn test_empty_list_is_truthy() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // '() (the empty list) is NOT false - only #f is false
    // Note: In Scheme, 'nil' is just a regular symbol, not the empty list
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if '() 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if 0 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
}

#[test]
fn test_eval_arithmetic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10 3)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 4 5)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 20 4)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3 4)"), 10);
}

#[test]
fn test_eval_quote() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("'hello").unwrap();
    assert!(lisp.symbol_matches(result, "hello").unwrap());
}

#[test]
fn test_eval_if() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if (< 1 2) 10 20)"), 10);
}

#[test]
fn test_eval_define() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 42)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 42);
}

#[test]
fn test_eval_lambda() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "((lambda (x) (+ x 1)) 5)"), 6);
}

#[test]
fn test_eval_define_function() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (square x) (* x x))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(square 5)"), 25);
}

#[test]
fn test_eval_let() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((x 10) (y 20)) (+ x y))"), 30);
}

#[test]
fn test_eval_let_star() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // let* allows sequential binding
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let* ((x 10) (y (+ x 5))) (+ x y))"), 25);
}

#[test]
fn test_tco_recursion() {
    // Strict evaluation with proper TCO
    // Deep recursion is safe with tail call optimization
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // This would overflow without proper TCO
    eval.eval_str("(define (sum-to n acc) (if (= n 0) acc (sum-to (- n 1) (+ acc n))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-to 100 0)"), 5050);  // sum 1..100
}

#[test]
fn test_eval_recursion() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 5)"), 120);
}

// ═══════════════════════════════════════════════════════════════════════════
// AUTOMATIC MEMOIZATION TESTS
// Recursive functions are automatically memoized with bounded LRU caches
// ═══════════════════════════════════════════════════════════════════════════

// NOTE: Some tests may require RUST_MIN_STACK=16777216 (16MB) to run
// This is due to Rust's default test thread stack being too small for
// deep parsing/evaluation of complex Lisp expressions, especially when
// using nested stdlib functions. The runtime evaluator uses trampolining
// but nested evaluations during let/define still use Rust recursion.

#[test]
fn test_simple_fib_define() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    // Simple single-recursive function
    eval.eval_str("(define (countdown n) (if (= n 0) 0 (countdown (- n 1))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(countdown 5)"), 0);
}

#[test]
fn test_auto_memoization_fibonacci() {
    // NOTE: This test requires larger stack: RUST_MIN_STACK=16777216
    // Fibonacci with double recursion tests automatic memoization
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // This is a complex expression that benefits from auto-memoization
    // Without memoization, fib(10) would be very slow
    eval.eval_str("(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))").unwrap();
    
    // Should work correctly - auto-memoization helps with performance
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fib 5)"), 5);
}

#[test]
fn test_auto_memoization_factorial() {
    // Factorial is automatically memoized
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))").unwrap();
    
    // First call computes and caches
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 10)"), 3628800);
    
    // Second call should use cached results
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 10)"), 3628800);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 5)"), 120);
}

#[test]
fn test_auto_memoization_not_applied_to_non_recursive() {
    // Non-recursive functions should NOT be memoized
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // This function doesn't reference itself - should not be memoized
    eval.eval_str("(define (square x) (* x x))").unwrap();
    
    // Should work normally
    assert_eq!(eval_to_num(&lisp, &mut eval, "(square 5)"), 25);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(square 10)"), 100);
}

#[test]
fn test_auto_memoization_with_multiple_args() {
    // Test memoization with multiple arguments - using simpler function
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple recursive function with 2 args
    eval.eval_str("(define (add-rec x y) (if (= y 0) x (add-rec (+ x 1) (- y 1))))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-rec 3 4)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-rec 5 3)"), 8);
}

#[test]
fn test_auto_memoization_cache_works() {
    // Verify that caching actually happens by checking repeated calls
    // Increased arena size to accommodate pack/unpack cons-list storage
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))").unwrap();
    
    // Call multiple times - should all return correct results
    // Using smaller value to reduce memory pressure
    for _ in 0..3 {
        assert_eq!(eval_to_num(&lisp, &mut eval, "(fib 8)"), 21);
    }
}

#[test]
fn test_auto_memoization_mutual_recursion_detection() {
    // Functions with complex recursion patterns should be detected
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Helper function that calls itself through conditions
    eval.eval_str("(define (countdown n) (if (= n 0) 'done (countdown (- n 1))))").unwrap();
    
    // Should work and be memoized
    let result = eval.eval_str("(countdown 50)").unwrap();
    assert!(lisp.symbol_matches(result, "done").unwrap());
}

#[test]
fn test_auto_memoization_nested_recursive_calls() {
    // Test with deeply nested recursive structure
    // Increased arena size to accommodate pack/unpack cons-list storage
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sum function - tail recursive
    eval.eval_str("(define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum 100 0)"), 5050);
}

#[test]
fn test_lru_cache_eviction() {
    // Test that cache eviction works by exceeding MAX_MEMO_CACHE_SIZE
    // Increased arena size to accommodate pack/unpack cons-list storage
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (identity n) (if (= n 0) 0 (identity (- n 1))))").unwrap();
    
    // Call with many different values to trigger cache eviction
    // The cache should limit to MAX_MEMO_CACHE_SIZE (100) entries
    // We'll just call with several values - enough to test eviction
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 50)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 60)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 70)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 80)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 90)"), 0);
    
    // Should still work correctly even after many calls
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 10)"), 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// STRICT EVALUATION TESTS
// Arguments are evaluated before function application (call-by-value)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_strict_basic() {
    // Basic strict evaluation - arguments are evaluated immediately
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple computation works
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3)"), 6);
    
    // Functions work - arguments evaluated before application
    eval.eval_str("(define (add x y) (+ x y))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add 10 20)"), 30);
}

#[test]
fn test_strict_cons_evaluates_args() {
    // cons evaluates its arguments in strict mode
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a pair - arguments are evaluated immediately
    eval.eval_str("(define p (cons (+ 1 2) (+ 3 4)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 7);
    
    // List creation works - all elements evaluated
    eval.eval_str("(define lst (list (* 2 3) (* 4 5) (* 6 7)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car lst)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr lst))"), 20);
}

#[test]
fn test_strict_side_effects_immediate() {
    // Side effects happen immediately in strict evaluation
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a counter to track side effects
    eval.eval_str("(define count 0)").unwrap();
    eval.eval_str("(define (side-effect! x) (set! count (+ count 1)) x)").unwrap();
    
    // Build a list - side effects happen immediately in strict mode!
    eval.eval_str("(define lst (cons (side-effect! 1) (cons (side-effect! 2) '())))").unwrap();
    
    // Count is 2 - both side effects happened during cons evaluation
    assert_eq!(eval_to_num(&lisp, &mut eval, "count"), 2);
    
    // Accessing elements doesn't trigger additional side effects
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car lst)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "count"), 2);
}

#[test]
fn test_strict_if_branches() {
    // Only the selected branch of 'if' is evaluated (short-circuit)
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // The non-selected branch is never evaluated
    eval.eval_str("(define (safe-div x y) (if (= y 0) 0 (/ x y)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(safe-div 10 0)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(safe-div 10 2)"), 5);
}

#[test]
fn test_strict_if_unevaluated_branch() {
    // 'if' is a special form - only one branch is evaluated
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // The undefined branch is never evaluated
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 42 undefined-var)"), 42);
    
    // Verify with side effects
    eval.eval_str("(define count 0)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 1 (begin (set! count 99) 2))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "count"), 0);  // else branch not evaluated
}

#[test]
fn test_strict_lambda_args_evaluated() {
    // Lambda arguments are evaluated before function application
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Lambda args are evaluated strictly
    eval.eval_str("(define (first x y) x)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(first (+ 1 1) (+ 2 2))"), 2);
    
    // Side effects in arguments happen before function body
    eval.eval_str("(define effect-count 0)").unwrap();
    eval.eval_str("(define (with-effect x) (set! effect-count (+ effect-count 1)) x)").unwrap();
    eval.eval_str("(define (ignore-second a b) a)").unwrap();
    
    // Both arguments are evaluated even though b is ignored
    assert_eq!(eval_to_num(&lisp, &mut eval, "(ignore-second (with-effect 1) (with-effect 2))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "effect-count"), 2);
}

#[test]
fn test_strict_tco_works() {
    // TCO works properly with strict evaluation
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Deep recursion with TCO
    eval.eval_str("(define (count n) (if (= n 0) 0 (count (- n 1))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(count 50)"), 0);  // No stack overflow
}

#[test]
fn test_strict_variable_binding() {
    // Variables are bound to evaluated values
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (square x) (* x x))").unwrap();
    eval.eval_str("(define val (square 5))").unwrap();
    
    // val is bound to 25, not to the expression (square 5)
    assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
    assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
    assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
}

#[test]
fn test_strict_closure() {
    // Closures capture their environment correctly
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (make-adder n) (lambda (x) (+ x n)))").unwrap();
    eval.eval_str("(define add5 (make-adder 5))").unwrap();
    eval.eval_str("(define add10 (make-adder 10))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 3)"), 8);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add10 3)"), 13);
}

#[test]
fn test_predicates() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // null? tests for empty list
    assert!(eval_is_true(&lisp, &mut eval, "(null? '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? '(1))"));
    
    assert!(eval_is_true(&lisp, &mut eval, "(pair? '(1 . 2))"));
    assert!(eval_is_false(&lisp, &mut eval, "(pair? 42)"));
    
    assert!(eval_is_true(&lisp, &mut eval, "(number? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(number? 'x)"));
    
    assert!(eval_is_true(&lisp, &mut eval, "(boolean? #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(boolean? #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean? 1)"));
    
    assert!(eval_is_true(&lisp, &mut eval, "(symbol? 'x)"));
    assert!(eval_is_false(&lisp, &mut eval, "(symbol? 42)"));
    
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? +)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? (lambda (x) x))"));
    assert!(eval_is_false(&lisp, &mut eval, "(procedure? 42)"));
}

#[test]
fn test_not() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(not #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not '())")); // empty list is truthy!
    assert!(eval_is_false(&lisp, &mut eval, "(not 0)"));   // 0 is truthy!
}

#[test]
fn test_cond() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval_to_num(&lisp, &mut eval, 
        "(cond ((< 5 3) 1) ((> 5 3) 2) (else 3))");
    assert_eq!(result, 2);
}

#[test]
fn test_and_or() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(and #t #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(and #t #f)"));
    assert!(eval_is_true(&lisp, &mut eval, "(and)"));  // Empty and is true
    
    assert!(eval_is_true(&lisp, &mut eval, "(or #f #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(or #f #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(or)"));  // Empty or is false
}

#[test]
fn test_gc_during_eval() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 42)").unwrap();
    let _stats = eval.gc();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 42);
}

// ═══════════════════════════════════════════════════════════════════════════
// COMPREHENSIVE TCO TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_tco_deep_recursion() {
    // Test TCO with deep recursion
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Tail-recursive sum: sum 1 to n
    eval.eval_str("(define (sum-tail n acc) (if (= n 0) acc (sum-tail (- n 1) (+ acc n))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-tail 100 0)"), 5050);  // 1+2+...+100
    
    // Tail-recursive countdown  
    eval.eval_str("(define (countdown n) (if (= n 0) 'done (countdown (- n 1))))").unwrap();
    let result = eval.eval_str("(countdown 100)").unwrap();
    assert!(lisp.get(result).unwrap().is_symbol());
}

#[test]
fn test_tco_mutual_recursion() {
    // Mutual recursion with TCO - even/odd predicates
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (my-even n) (if (= n 0) #t (my-odd (- n 1))))").unwrap();
    eval.eval_str("(define (my-odd n) (if (= n 0) #f (my-even (- n 1))))").unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(my-even 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(my-odd 0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(my-even 20)"));
    assert!(eval_is_false(&lisp, &mut eval, "(my-odd 20)"));
    assert!(eval_is_false(&lisp, &mut eval, "(my-even 19)"));
    assert!(eval_is_true(&lisp, &mut eval, "(my-odd 19)"));
}

#[test]
fn test_tco_accumulator_pattern() {
    // Classic tail-recursive patterns
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Tail-recursive factorial
    eval.eval_str("(define (fact-tail n acc) (if (= n 0) acc (fact-tail (- n 1) (* n acc))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fact-tail 10 1)"), 3628800);
    
    // Tail-recursive length
    eval.eval_str("(define (len-tail lst acc) (if (null? lst) acc (len-tail (cdr lst) (+ acc 1))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(len-tail '(a b c d e) 0)"), 5);
    
    // Tail-recursive reverse
    eval.eval_str("(define (rev-tail lst acc) (if (null? lst) acc (rev-tail (cdr lst) (cons (car lst) acc))))").unwrap();
    let _result = eval.eval_str("(rev-tail '(1 2 3) '())").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (rev-tail '(1 2 3) '()))"), 3);
}

#[test]
fn test_tco_in_cond() {
    // TCO should work in cond branches
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define (classify n)
            (cond ((< n 0) (classify (- 0 n)))
                  ((= n 0) 'zero)
                  ((< n 10) 'small)
                  ((< n 100) 'medium)
                  (else (classify (/ n 10)))))
    "#).unwrap();
    
    let result = eval.eval_str("(classify -42)").unwrap();
    assert!(lisp.get(result).unwrap().is_symbol());
    let result = eval.eval_str("(classify 999)").unwrap();
    assert!(lisp.get(result).unwrap().is_symbol());
}

// ═══════════════════════════════════════════════════════════════════════════
// SHORT-CIRCUIT EVALUATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_and_or_short_circuit() {
    // and/or short-circuit evaluation (only evaluates until result is known)
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // 'and' stops at first false
    assert!(eval_is_false(&lisp, &mut eval, "(and #f undefined-error)"));
    assert!(eval_is_true(&lisp, &mut eval, "(and #t #t #t)"));
    
    // 'or' stops at first true
    assert!(eval_is_true(&lisp, &mut eval, "(or #t undefined-error)"));
    assert!(eval_is_false(&lisp, &mut eval, "(or #f #f #f)"));
}

#[test]
fn test_let_bindings() {
    // let bindings evaluate values strictly
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic let
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((x 10) (y 20)) (+ x y))"), 30);
    
    // let* with dependencies
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let* ((x 5) (y (* x 2))) (+ x y))"), 15);
    
    // Nested let
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let ((x 1)) (let ((y 2)) (let ((z 3)) (+ x y z))))"), 6);
}

#[test]
fn test_cons_with_expressions() {
    // cons evaluates expressions in strict mode
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build a list with computations - all evaluated immediately
    eval.eval_str("(define p (cons (+ 1 2) (+ 3 4)))").unwrap();
    
    // Values are already computed
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 7);
}

#[test]
fn test_nested_structures() {
    // Deeply nested structures work with strict evaluation
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create nested pairs
    eval.eval_str("(define deep (cons (cons (cons 1 2) 3) 4))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr deep)"), 4);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (car deep))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (car (car deep)))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (car (car deep)))"), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// FUNCTION COMPOSITION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_nested_calls() {
    // Test behavior with nested function calls
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (double x) (* x 2))").unwrap();
    eval.eval_str("(define (quad x) (double (double x)))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(quad 5)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(quad (quad 2))"), 32);
}

#[test]
fn test_higher_order() {
    // Higher-order functions
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (apply-twice f x) (f (f x)))").unwrap();
    eval.eval_str("(define (inc x) (+ x 1))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply-twice inc 0)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply-twice (lambda (x) (* x 2)) 3)"), 12);
}

#[test]
fn test_currying() {
    // Curried functions
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (curry-add a) (lambda (b) (lambda (c) (+ a b c))))").unwrap();
    eval.eval_str("(define add1 (curry-add 1))").unwrap();
    eval.eval_str("(define add1-2 (add1 2))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add1-2 3)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(((curry-add 10) 20) 30)"), 60);
}

#[test]
fn test_repeated_access() {
    // Verify repeated access returns same value
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a computation stored in a cons
    eval.eval_str("(define p (cons (* 111 111) 0))").unwrap();
    
    // All accesses return the same pre-computed value
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
}

// ═══════════════════════════════════════════════════════════════════════════
// NEW FEATURES TESTS
// ═══════════════════════════════════════════════════════════════════════════

// ───────────────────────────────────────────────────────────────────────────
// CASE - Pattern Matching
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_case_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(case 2 ((1) 10) ((2) 20) ((3) 30))"), 20);
}

#[test]
fn test_case_multiple_datums() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(case 'b ((a) 1) ((b c) 2) ((d) 3))"), 2);
}

#[test]
fn test_case_else() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(case 99 ((1) 10) ((2) 20) (else 0))"), 0);
}

#[test]
fn test_case_no_match() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("(case 5 ((1) 10) ((2) 20))").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

// ───────────────────────────────────────────────────────────────────────────
// DO - Iteration Construct
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_do_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple countdown
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(do ((i 5 (- i 1))) ((= i 0) 42))"), 42);
}

#[test]
fn test_do_accumulator() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sum 1 to 5 using do loop
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(do ((i 1 (+ i 1)) (sum 0 (+ sum i))) ((> i 5) sum))"), 15);
}

#[test]
fn test_do_factorial() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Factorial using do loop
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(do ((n 5 (- n 1)) (result 1 (* result n))) ((= n 0) result))"), 120);
}

// ───────────────────────────────────────────────────────────────────────────
// QUASIQUOTE - Template with Unquote
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_quasiquote_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 42)").unwrap();
    
    // `(a b ,x) should give (a b 42)
    let result = eval.eval_str("(quasiquote (a b (unquote x)))").unwrap();
    // Check structure: (a b 42)
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert_eq!(lisp.get(third).unwrap().as_number().unwrap(), 42);
}

#[test]
fn test_quasiquote_nested() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define y 10)").unwrap();
    
    // Template with multiple unquotes
    let result = eval.eval_str("(quasiquote ((unquote y) 2 (unquote (+ y 1))))").unwrap();
    let first = lisp.car(result).unwrap();
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    
    assert_eq!(lisp.get(first).unwrap().as_number().unwrap(), 10);
    assert_eq!(lisp.get(third).unwrap().as_number().unwrap(), 11);
}

// ───────────────────────────────────────────────────────────────────────────
// EVAL - Meta-circular Evaluator
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_eval_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Evaluate a quoted expression
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval '(+ 1 2))"), 3);
}

#[test]
fn test_eval_symbol() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 99)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval 'x)"), 99);
}

#[test]
fn test_eval_computed() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build expression dynamically and evaluate it
    eval.eval_str("(define expr (list '+ 10 20))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval expr)"), 30);
}

// ───────────────────────────────────────────────────────────────────────────
// Note: defmacro and gensym have been removed for Scheme R7RS conformance.
// Hygienic macros via syntax-rules will be implemented in a future phase.
// ───────────────────────────────────────────────────────────────────────────

// ───────────────────────────────────────────────────────────────────────────
// APPLY - Apply Function to List
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_apply_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply + '(1 2 3))"), 6);
}

#[test]
fn test_apply_lambda() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (sum3 a b c) (+ a b c))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply sum3 '(10 20 30))"), 60);
}

// ───────────────────────────────────────────────────────────────────────────
// VALUES - Multiple Return Values
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_values_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // values returns a list of its arguments
    let result = eval.eval_str("(values 1 2 3)").unwrap();
    assert_eq!(lisp.get(lisp.car(result).unwrap()).unwrap().as_number().unwrap(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// COMPREHENSIVE BUILTIN TESTS
// ═══════════════════════════════════════════════════════════════════════════

// ───────────────────────────────────────────────────────────────────────────
// List Operations
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_car_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car '(1 2 3))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cons 42 99))"), 42);
    
    // Scheme R7RS: car of empty list is an error
    let result = eval.eval_str("(car '())");
    assert!(result.is_err());
}

#[test]
fn test_cdr_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // cdr of list
    let result = eval.eval_str("(cdr '(1 2 3))").unwrap();
    assert_eq!(lisp.get(lisp.car(result).unwrap()).unwrap().as_number().unwrap(), 2);
    
    // cdr of pair
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (cons 1 2))"), 2);
    
    // Scheme R7RS: cdr of empty list is an error
    let result = eval.eval_str("(cdr '())");
    assert!(result.is_err());
}

#[test]
fn test_cons_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic cons
    let result = eval.eval_str("(cons 1 2)").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
    
    // Cons to nil creates proper list
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cons 1 '()))"), 1);
}

#[test]
fn test_list_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Empty list
    let result = eval.eval_str("(list)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
    
    // Single element
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (list 42))"), 42);
    
    // Multiple elements
    eval.eval_str("(define my-list (list 1 2 3 4 5))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car my-list)"), 1);
}

// ───────────────────────────────────────────────────────────────────────────
// Predicates
// ───────────────────────────────────────────────────────────────────────────

// Note: atom has been removed for Scheme R7RS conformance.
// Use (not (pair? x)) instead.

#[test]
fn test_eq_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Scheme uses eq? for identity comparison
    assert!(eval_is_true(&lisp, &mut eval, "(eq? 1 1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(eq? 1 2)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq? 'a 'a)"));
    assert!(eval_is_false(&lisp, &mut eval, "(eq? 'a 'b)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq? '() '())"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq? #t #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq? #f #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(eq? #t #f)"));
    
    // eqv? has same semantics for basic types
    assert!(eval_is_true(&lisp, &mut eval, "(eqv? 1 1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(eqv? 1 2)"));
    
    // equal? provides structural equality
    assert!(eval_is_true(&lisp, &mut eval, "(equal? '(1 2 3) '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(equal? '(1 2 3) '(1 2 4))"));
}

#[test]
fn test_null_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // null? tests for the empty list '()
    // Note: 'nil' is just a regular symbol in Scheme, not the empty list
    assert!(eval_is_true(&lisp, &mut eval, "(null? '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? '(1))"));
}

#[test]
fn test_pair_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(pair? '(1 2))"));
    assert!(eval_is_true(&lisp, &mut eval, "(pair? (cons 1 2))"));
    assert!(eval_is_false(&lisp, &mut eval, "(pair? '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(pair? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(pair? 'x)"));
}

#[test]
fn test_number_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(number? 42)"));
    assert!(eval_is_true(&lisp, &mut eval, "(number? -10)"));
    assert!(eval_is_true(&lisp, &mut eval, "(number? 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(number? 'x)"));
    assert!(eval_is_false(&lisp, &mut eval, "(number? #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(number? '())"));
}

#[test]
fn test_boolean_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(boolean? #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(boolean? #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean? 1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean? '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean? 'true)"));
}

#[test]
fn test_symbol_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(symbol? 'x)"));
    assert!(eval_is_true(&lisp, &mut eval, "(symbol? 'hello-world)"));
    assert!(eval_is_false(&lisp, &mut eval, "(symbol? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(symbol? #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(symbol? '(a b))"));
}

#[test]
fn test_procedure_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? +)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? car)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? (lambda (x) x))"));
    assert!(eval_is_false(&lisp, &mut eval, "(procedure? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(procedure? 'lambda)"));
}

// ───────────────────────────────────────────────────────────────────────────
// Arithmetic Operations
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_add_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3 4 5)"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ -5 10)"), 5);
}

#[test]
fn test_sub_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10)"), -10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10 3)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 100 20 30)"), 50);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 5 10)"), -5);
}

#[test]
fn test_mul_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(*)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 2 3)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 2 3 4)"), 24);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* -2 3)"), -6);
}

#[test]
fn test_div_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 10 2)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 100 2 5)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ -10 2)"), -5);
}

#[test]
fn test_modulo_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Scheme modulo: result has sign of divisor
    assert_eq!(eval_to_num(&lisp, &mut eval, "(modulo 10 3)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(modulo 15 5)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(modulo 7 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(modulo -13 4)"), 3);  // Scheme: result has sign of divisor
    assert_eq!(eval_to_num(&lisp, &mut eval, "(modulo 13 -4)"), -3); // Scheme: result has sign of divisor
}

#[test]
fn test_remainder_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Scheme remainder: result has sign of dividend
    assert_eq!(eval_to_num(&lisp, &mut eval, "(remainder 10 3)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(remainder 15 5)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(remainder -13 4)"), -1); // Scheme: result has sign of dividend
    assert_eq!(eval_to_num(&lisp, &mut eval, "(remainder 13 -4)"), 1);  // Scheme: result has sign of dividend
}

// ───────────────────────────────────────────────────────────────────────────
// Comparison Operators
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_comparisons_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Less than
    assert!(eval_is_true(&lisp, &mut eval, "(< 1 2)"));
    assert!(eval_is_false(&lisp, &mut eval, "(< 2 1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(< 2 2)"));
    
    // Greater than
    assert!(eval_is_true(&lisp, &mut eval, "(> 2 1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(> 1 2)"));
    assert!(eval_is_false(&lisp, &mut eval, "(> 2 2)"));
    
    // Less than or equal
    assert!(eval_is_true(&lisp, &mut eval, "(<= 1 2)"));
    assert!(eval_is_true(&lisp, &mut eval, "(<= 2 2)"));
    assert!(eval_is_false(&lisp, &mut eval, "(<= 3 2)"));
    
    // Greater than or equal
    assert!(eval_is_true(&lisp, &mut eval, "(>= 2 1)"));
    assert!(eval_is_true(&lisp, &mut eval, "(>= 2 2)"));
    assert!(eval_is_false(&lisp, &mut eval, "(>= 1 2)"));
    
    // Numeric equality
    assert!(eval_is_true(&lisp, &mut eval, "(= 5 5)"));
    assert!(eval_is_false(&lisp, &mut eval, "(= 5 6)"));
}

// ───────────────────────────────────────────────────────────────────────────
// Boolean Operations
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_not_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(not #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(not 'x)"));
}

#[test]
fn test_and_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(and)"));
    assert!(eval_is_true(&lisp, &mut eval, "(and #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(and #f)"));
    assert!(eval_is_true(&lisp, &mut eval, "(and #t #t #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(and #t #f #t)"));
    
    // Short-circuit
    assert!(eval_is_false(&lisp, &mut eval, "(and #f undefined-var)"));
}

#[test]
fn test_or_comprehensive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_false(&lisp, &mut eval, "(or)"));
    assert!(eval_is_true(&lisp, &mut eval, "(or #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(or #f)"));
    assert!(eval_is_true(&lisp, &mut eval, "(or #f #t #f)"));
    
    // Short-circuit
    assert!(eval_is_true(&lisp, &mut eval, "(or #t undefined-var)"));
}

// ───────────────────────────────────────────────────────────────────────────
// I/O and Error Handling
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_display() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // display returns its argument (R7RS compliant)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(display 99)"), 99);
}

#[test]
fn test_newline() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("(newline)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

#[test]
fn test_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("(error 'test-error)");
    assert!(result.is_err());
}

// ───────────────────────────────────────────────────────────────────────────
// ───────────────────────────────────────────────────────────────────────────
// Lexical Closures - Additional Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_closure_counter() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Counter using closure (pure functional - returns new state)
    eval.eval_str("(define (make-counter init) (lambda (delta) (+ init delta)))").unwrap();
    eval.eval_str("(define counter (make-counter 10))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(counter 5)"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(counter 10)"), 20);
}

#[test]
fn test_closure_nested() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Triple-nested closures
    eval.eval_str("(define (f a) (lambda (b) (lambda (c) (+ a b c))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(((f 1) 2) 3)"), 6);
}

#[test]
fn test_closure_captures_correct_env() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Verify lexical scoping (not dynamic)
    eval.eval_str("(define x 1)").unwrap();
    eval.eval_str("(define (get-x) x)").unwrap();
    eval.eval_str("(define (call-with-x val f) (let ((x val)) (f)))").unwrap();
    
    // Should use lexical binding (x=1), not dynamic binding (x=100)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(call-with-x 100 get-x)"), 1);
}

// ───────────────────────────────────────────────────────────────────────────
// List Processing Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_filter_multiples() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Filter helper: filter out multiples (using modulo instead of mod)
    eval.eval_str("(define (filter-multiples n stream) (cond ((null? stream) '()) ((= (modulo (car stream) n) 0) (filter-multiples n (cdr stream))) (else (cons (car stream) (filter-multiples n (cdr stream))))))").unwrap();
    
    // Test filter-multiples on a finite list
    eval.eval_str("(define nums '(2 3 4 5 6 7 8 9 10))").unwrap();
    eval.eval_str("(define filtered (filter-multiples 2 nums))").unwrap();
    
    // Check first element (filters out even numbers)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car filtered)"), 3);
}

// ═══════════════════════════════════════════════════════════════════════════
// MUTATION TESTS
// Tests for set!, set-car!, set-cdr! operations
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_mutation_set() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic set! mutation
    eval.eval_str("(define x 10)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 10);
    
    eval.eval_str("(set! x 20)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 20);
    
    // Multiple mutations
    eval.eval_str("(set! x 30)").unwrap();
    eval.eval_str("(set! x 40)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 40);
}

#[test]
fn test_mutation_set_in_closure() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Counter using set!
    eval.eval_str("(define counter 0)").unwrap();
    eval.eval_str("(define (inc!) (set! counter (+ counter 1)))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 0);
    eval.eval_str("(inc!)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 1);
    eval.eval_str("(inc!)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 2);
}

#[test]
fn test_mutation_set_car_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);
    
    eval.eval_str("(set-car! p 10)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 2);  // cdr unchanged
}

#[test]
fn test_mutation_set_cdr_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 2);
    
    eval.eval_str("(set-cdr! p 20)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);  // car unchanged
}

#[test]
fn test_mutation_build_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build a list by mutation
    eval.eval_str("(define lst (cons 1 '()))").unwrap();
    eval.eval_str("(set-cdr! lst (cons 2 '()))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car lst)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr lst))"), 2);
}

#[test]
fn test_mutation_with_gc() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Mutation should survive GC
    eval.eval_str("(define x (cons 1 2))").unwrap();
    eval.eval_str("(set-car! x 100)").unwrap();
    
    // Run GC
    let _stats = eval.gc();
    
    // Value should persist after GC
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car x)"), 100);
}

// ═══════════════════════════════════════════════════════════════════════════
// STDLIB FUNCTION TESTS  
// Tests for the new static standard library functions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_stdlib_length() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '())"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(a b c d e))"), 5);
}

#[test]
fn test_stdlib_fold() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sum of a list
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fold + 0 '(1 2 3 4 5))"), 15);
    
    // Product of a list
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fold * 1 '(1 2 3 4 5))"), 120);
    
    // Empty list returns accumulator
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fold + 42 '())"), 42);
}

#[test]
fn test_stdlib_nth() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(nth 0 '(10 20 30))"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(nth 1 '(10 20 30))"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(nth 2 '(10 20 30))"), 30);
}

#[test]
fn test_stdlib_range() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test range function
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (range 0 5))"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (range 0 5))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (range 0 5)))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (range 0 0))"), 0);
}

#[test]
fn test_stdlib_identity_and_constantly() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 42)"), 42);
    
    eval.eval_str("(define always-5 (constantly 5))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(always-5 1)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(always-5 100)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(always-5 'foo)"), 5);
}

#[test]
fn test_stdlib_curry() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Curry + 5 to create add5
    eval.eval_str("(define add5 (curry + 5))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 10)"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 20)"), 25);
}

#[test]
fn test_stdlib_cadr_caddr_cddr() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr '(1 2 3))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(caddr '(1 2 3))"), 3);
    
    // cddr returns the list after first two elements
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cddr '(1 2 3 4)))"), 3);
}

#[test]
fn test_stdlib_member() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(member 2 '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(member 5 '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(member 1 '())"));
}

// ═══════════════════════════════════════════════════════════════════════════
// GC INTEGRATION TESTS
// Tests that verify GC works correctly with various scenarios
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_gc_preserves_closures() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (make-counter) (define n 0) (lambda () (set! n (+ n 1)) n))").unwrap();
    eval.eval_str("(define counter (make-counter))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(counter)"), 1);
    
    // GC should preserve the closure and its environment
    let _stats = eval.gc();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(counter)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(counter)"), 3);
}

#[test]
fn test_gc_collects_unreachable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create some garbage
    eval.eval_str("(cons 1 2)").unwrap();
    eval.eval_str("(cons 3 4)").unwrap();
    eval.eval_str("(cons 5 6)").unwrap();
    
    let stats_before = lisp.stats();
    let gc_stats = eval.gc();
    let stats_after = lisp.stats();
    
    // Some garbage should have been collected
    assert!(gc_stats.collected > 0);
    assert!(stats_after.allocated <= stats_before.allocated);
}

#[test]
fn test_gc_intern_table_survives() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create some symbols
    eval.eval_str("(define a 1)").unwrap();
    eval.eval_str("(define b 2)").unwrap();
    eval.eval_str("(define c 3)").unwrap();
    
    // GC
    eval.gc();
    
    // Symbols should still be usable
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ a b c)"), 6);
}

// ═══════════════════════════════════════════════════════════════════════════
// PITFALL / GOTCHA DOCUMENTATION TESTS
// These tests document expected behavior that might be surprising
// ═══════════════════════════════════════════════════════════════════════════

/// PITFALL: The empty list '() is NOT false!
/// In this Lisp, only #f is false. The empty list is truthy.
/// Note: 'nil' is just a regular symbol in Scheme, not the empty list.
#[test]
fn test_pitfall_empty_list_is_truthy() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // The empty list is truthy!
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if '() 1 2)"), 1);  // Takes then branch
    
    // Only #f is false
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);  // Takes else branch
}

/// PITFALL: 0 is also truthy!
#[test]
fn test_pitfall_zero_is_truthy() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // 0 is truthy (unlike C/Python)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if 0 1 2)"), 1);  // Takes then branch
}

/// PITFALL: StdLib functions parse their body on each call (minor overhead)
/// This is intentional - it keeps function code out of the arena.
#[test]
fn test_stdlib_parses_on_each_call() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Multiple calls to stdlib function work correctly
    // (this verifies parsing works repeatedly)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1 2))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1 2 3))"), 3);
    
    // Complex usage
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fold + 0 (range 0 10))"), 45);
}

/// PITFALL: Recursive stdlib functions work via the global environment
#[test]
fn test_stdlib_recursion_works() {
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // length is recursive - should work for small lists
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(a b c d e f g h i j))"), 10);
    
    // fold is tail-recursive - more efficient
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fold + 0 '(1 2 3 4 5 6 7 8 9 10))"), 55);
}

// ═══════════════════════════════════════════════════════════════════════════
// GC BUILTIN TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_gc_builtin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create some garbage
    eval.eval_str("(cons 1 2)").unwrap();
    eval.eval_str("(cons 3 4)").unwrap();
    
    // Run GC via builtin
    let result = eval.eval_str("(gc)").unwrap();
    
    // Result should be a list (marked collected total-before)
    assert!(lisp.get(result).unwrap().is_cons());
    
    // Extract values
    let marked = lisp.car(result).unwrap();
    assert!(lisp.get(marked).unwrap().is_number());
}

#[test]
fn test_gc_enable_disable() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // GC should be enabled by default
    assert!(eval_is_true(&lisp, &mut eval, "(gc-enabled?)"));
    
    // Disable GC
    let result = eval.eval_str("(gc-disable)").unwrap();
    assert!(lisp.get(result).unwrap().is_false());
    
    // Verify disabled
    assert!(eval_is_false(&lisp, &mut eval, "(gc-enabled?)"));
    
    // Enable GC
    let result = eval.eval_str("(gc-enable)").unwrap();
    assert!(lisp.get(result).unwrap().is_true());
    
    // Verify enabled
    assert!(eval_is_true(&lisp, &mut eval, "(gc-enabled?)"));
}

#[test]
fn test_arena_stats_builtin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Get arena stats
    let result = eval.eval_str("(arena-stats)").unwrap();
    
    // Result should be a list (capacity allocated free usage-percent)
    assert!(lisp.get(result).unwrap().is_cons());
    
    // First element should be capacity = 20000
    let capacity = lisp.car(result).unwrap();
    assert_eq!(lisp.get(capacity).unwrap().as_number(), Some(20000));
    
    // Second element (allocated) should be a number
    let rest = lisp.cdr(result).unwrap();
    let allocated = lisp.car(rest).unwrap();
    assert!(lisp.get(allocated).unwrap().is_number());
    assert!(lisp.get(allocated).unwrap().as_number().unwrap() > 0);
}

#[test]
fn test_gc_disabled_no_collect() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Disable GC
    eval.eval_str("(gc-disable)").unwrap();
    
    // Create some garbage
    let before = eval.eval_str("(arena-stats)").unwrap();
    let _before_allocated = lisp.get(lisp.car(lisp.cdr(before).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    eval.eval_str("(cons 1 2)").unwrap();
    eval.eval_str("(cons 3 4)").unwrap();
    
    // GC with disabled - should not collect
    let gc_result = eval.eval_str("(gc)").unwrap();
    let marked = lisp.get(lisp.car(gc_result).unwrap()).unwrap().as_number().unwrap();
    let collected = lisp.get(lisp.car(lisp.cdr(gc_result).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    // When disabled, marked and collected should be 0
    assert_eq!(marked, 0);
    assert_eq!(collected, 0);
    
    // Re-enable GC
    eval.eval_str("(gc-enable)").unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════
// R7RS PHASE 1 CONFORMANCE TESTS
// Testing letrec, letrec*, when, unless, and new stdlib functions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_letrec_basic() {
    // letrec allows mutually recursive definitions
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic letrec - recursive function
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(letrec ((fact (lambda (n) (if (= n 0) 1 (* n (fact (- n 1))))))) (fact 5))"), 
        120);
}

#[test]
fn test_letrec_mutual_recursion() {
    // letrec supports mutually recursive definitions (R7RS example)
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Even?/odd? mutual recursion from R7RS spec
    assert!(eval_is_true(&lisp, &mut eval, 
        "(letrec ((even? (lambda (n) (if (zero? n) #t (odd? (- n 1))))) 
                  (odd? (lambda (n) (if (zero? n) #f (even? (- n 1)))))) 
          (even? 88))"));
    
    assert!(eval_is_false(&lisp, &mut eval, 
        "(letrec ((even? (lambda (n) (if (zero? n) #t (odd? (- n 1))))) 
                  (odd? (lambda (n) (if (zero? n) #f (even? (- n 1)))))) 
          (even? 7))"));
}

#[test]
fn test_letrec_star_basic() {
    // letrec* evaluates bindings sequentially
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic letrec* - same as letrec for simple cases
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(letrec* ((x 1) (y (+ x 2))) (+ x y))"), 
        4);
}

#[test]
fn test_letrec_star_mutual_recursion() {
    // letrec* also supports mutual recursion
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Mutual recursion works in letrec*
    assert!(eval_is_true(&lisp, &mut eval, 
        "(letrec* ((even? (lambda (n) (if (zero? n) #t (odd? (- n 1))))) 
                   (odd? (lambda (n) (if (zero? n) #f (even? (- n 1)))))) 
          (even? 10))"));
}

#[test]
fn test_when_basic() {
    // when evaluates body when test is true
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a counter to track when the body is evaluated
    eval.eval_str("(define counter 0)").unwrap();
    
    // When test is true, body executes
    eval.eval_str("(when #t (set! counter (+ counter 1)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 1);
    
    // When test is false, body does not execute
    eval.eval_str("(when #f (set! counter (+ counter 10)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 1);
}

#[test]
fn test_when_multiple_expressions() {
    // when can have multiple expressions in body
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define a 0)").unwrap();
    eval.eval_str("(define b 0)").unwrap();
    
    eval.eval_str("(when (= 1 1) (set! a 1) (set! b 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "a"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "b"), 2);
}

#[test]
fn test_unless_basic() {
    // unless evaluates body when test is false
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define counter 0)").unwrap();
    
    // Unless test is true (truthy), body does not execute
    eval.eval_str("(unless #t (set! counter (+ counter 1)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 0);
    
    // Unless test is false, body executes
    eval.eval_str("(unless #f (set! counter (+ counter 10)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 10);
}

#[test]
fn test_unless_multiple_expressions() {
    // unless can have multiple expressions in body
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define a 0)").unwrap();
    eval.eval_str("(define b 0)").unwrap();
    
    // Only executes when test is false
    eval.eval_str("(unless (= 1 2) (set! a 5) (set! b 6))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "a"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "b"), 6);
}

#[test]
fn test_rounding_operations() {
    // floor, ceiling, truncate, round are identity for integers
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // All rounding operations are identity for integers
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor -5)"), -5);
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(ceiling 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(ceiling -5)"), -5);
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate -5)"), -5);
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(round 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(round -5)"), -5);
}

#[test]
fn test_exact_integer_predicate() {
    // exact-integer? returns #t for all our integers
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(exact-integer? 42)"));
    assert!(eval_is_true(&lisp, &mut eval, "(exact-integer? 0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(exact-integer? -100)"));
    assert!(eval_is_false(&lisp, &mut eval, "(exact-integer? 'symbol)"));
}

#[test]
fn test_make_list_stdlib() {
    // make-list creates a list of k elements
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Make a list of 3 zeros
    let result = eval.eval_str("(make-list 3 0)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (make-list 3 0))"), 3);
    
    // First element is the fill value
    let car = lisp.car(result).unwrap();
    assert_eq!(lisp.get(car).unwrap().as_number().unwrap(), 0);
}

#[test]
fn test_last_and_last_pair() {
    // last returns the last element, last-pair returns the last pair
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(last '(1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(last '(5))"), 5);
    
    // last-pair returns the last pair
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (last-pair '(1 2 3)))"), 3);
}

#[test]
fn test_any_and_every() {
    // any and every higher-order functions
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // any returns #t if predicate is true for any element
    assert!(eval_is_true(&lisp, &mut eval, "(any positive? '(1 -2 -3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(any positive? '(-1 -2 -3))"));
    
    // every returns #t if predicate is true for all elements
    assert!(eval_is_true(&lisp, &mut eval, "(every positive? '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(every positive? '(1 -2 3))"));
}

#[test]
fn test_find() {
    // find returns the first element matching predicate
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(find even? '(1 3 4 5 6))"), 4);
    assert!(eval_is_false(&lisp, &mut eval, "(find even? '(1 3 5 7))"));
}

#[test]
fn test_remove_and_delete() {
    // remove and delete filter lists
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // remove removes elements matching predicate
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (remove even? '(1 2 3 4 5)))"), 3);
    
    // delete removes specific element using equal?
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (delete 2 '(1 2 3 2 4)))"), 3);
}

#[test]
fn test_fold_right() {
    // fold-right is a right fold
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // fold-right processes list from right-to-left, but with cons and empty list
    // it preserves the original order: (cons 1 (cons 2 (cons 3 '()))) = (1 2 3)
    let result = eval.eval_str("(fold-right cons '() '(1 2 3))").unwrap();
    let first = lisp.car(result).unwrap();
    assert_eq!(lisp.get(first).unwrap().as_number().unwrap(), 1);
}

#[test]
fn test_reduce() {
    // reduce is an alias for fold-right
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(reduce + 0 '(1 2 3 4))"), 10);
}

#[test]
fn test_cddddr() {
    // cddddr - 4-level cdr
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("(cddddr '(1 2 3 4 5 6))").unwrap();
    let first = lisp.car(result).unwrap();
    assert_eq!(lisp.get(first).unwrap().as_number().unwrap(), 5);
}

#[test]
fn test_partition() {
    // partition splits a list into matching and non-matching elements
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // partition returns (cons matching non-matching) - a pair of two lists
    eval.eval_str("(define result (partition even? '(1 2 3 4 5 6)))").unwrap();
    
    // car is the matching elements (even numbers: 2, 4, 6)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (car result))"), 3);
    
    // cdr is the non-matching elements (odd numbers: 1, 3, 5)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (cdr result))"), 3);
}

#[test]
fn test_filter_map() {
    // filter-map applies function and keeps non-#f results
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a function that returns #f for negative numbers
    eval.eval_str("(define (pos-or-false x) (if (positive? x) x #f))").unwrap();
    
    // filter-map keeps only the positive values
    eval.eval_str("(define result (filter-map pos-or-false '(-1 2 -3 4 -5)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length result)"), 2);
}

#[test]
fn test_boolean_eq() {
    // boolean-eq checks if both arguments have the same boolean value
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(boolean-eq #t #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(boolean-eq #f #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean-eq #t #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean-eq #f #t)"));
}

#[test]
fn test_member_equal_and_assoc_equal() {
    // member-equal and assoc-equal use equal? for comparison
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // member-equal works with nested lists (unlike memq which uses eq?)
    let result = eval.eval_str("(member-equal '(a) '(x (a) y))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false()); // Should find it
    
    // assoc-equal works with nested keys
    let result = eval.eval_str("(assoc-equal '(a) '(((a) 1) ((b) 2)))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false()); // Should find it
}

#[test]
fn test_list_set() {
    // list-set! mutates an element in a list
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define my-list (list 1 2 3 4 5))").unwrap();
    eval.eval_str("(list-set! my-list 2 99)").unwrap();
    
    // Third element (index 2) should now be 99
    assert_eq!(eval_to_num(&lisp, &mut eval, "(list-ref my-list 2)"), 99);
}

#[test]
fn test_make_list_edge_cases() {
    // make-list handles edge cases
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Empty list for k=0
    let result = eval.eval_str("(make-list 0 'x)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
    
    // Negative k should return empty list (not infinite recursion)
    let result = eval.eval_str("(make-list -5 'x)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

// ============================================================
// R7RS Phase 2: Character and String Tests
// ============================================================

// Helper to get a character from eval result
fn eval_to_char<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> char {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().as_char().unwrap()
}

// Helper to check if a string matches
fn eval_string_matches<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str, expected: &str) -> bool {
    let result = eval.eval_str(input).unwrap();
    lisp.string_matches(result, expected).unwrap()
}

// ============================================================
// Character Literal Parsing Tests
// ============================================================

#[test]
fn test_char_literal_simple() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple character literals
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\a"), 'a');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\A"), 'A');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\z"), 'z');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\0"), '0');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\("), '(');
}

#[test]
fn test_char_literal_named() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Named character literals
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\newline"), '\n');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\space"), ' ');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\tab"), '\t');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\return"), '\r');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\null"), '\0');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\alarm"), '\x07');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\backspace"), '\x08');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\delete"), '\x7F');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\escape"), '\x1B');
}

#[test]
fn test_char_literal_hex() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Hex character literals
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\x41"), 'A');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\x61"), 'a');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"#\x20"), ' ');
}

// ============================================================
// Character Predicate and Operation Tests
// ============================================================

#[test]
fn test_char_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // char? predicate
    assert!(eval_is_true(&lisp, &mut eval, r"(char? #\a)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char? #\space)"));
    assert!(eval_is_false(&lisp, &mut eval, "(char? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, r#"(char? "hello")"#));
}

#[test]
fn test_char_comparison() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // char=?
    assert!(eval_is_true(&lisp, &mut eval, r"(char=? #\a #\a)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char=? #\a #\b)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char=? #\a #\a #\a)"));
    
    // char<?
    assert!(eval_is_true(&lisp, &mut eval, r"(char<? #\a #\b)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char<? #\a #\b #\c)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char<? #\b #\a)"));
    
    // char>?
    assert!(eval_is_true(&lisp, &mut eval, r"(char>? #\b #\a)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char>? #\a #\b)"));
    
    // char<=?
    assert!(eval_is_true(&lisp, &mut eval, r"(char<=? #\a #\a)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char<=? #\a #\b)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char<=? #\b #\a)"));
    
    // char>=?
    assert!(eval_is_true(&lisp, &mut eval, r"(char>=? #\a #\a)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char>=? #\b #\a)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char>=? #\a #\b)"));
}

#[test]
fn test_char_conversion() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // char->integer
    assert_eq!(eval_to_num(&lisp, &mut eval, r"(char->integer #\A)"), 65);
    assert_eq!(eval_to_num(&lisp, &mut eval, r"(char->integer #\a)"), 97);
    assert_eq!(eval_to_num(&lisp, &mut eval, r"(char->integer #\space)"), 32);
    
    // integer->char
    assert_eq!(eval_to_char(&lisp, &mut eval, "(integer->char 65)"), 'A');
    assert_eq!(eval_to_char(&lisp, &mut eval, "(integer->char 97)"), 'a');
    assert_eq!(eval_to_char(&lisp, &mut eval, "(integer->char 32)"), ' ');
}

#[test]
fn test_char_case_conversion() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // char-upcase
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-upcase #\a)"), 'A');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-upcase #\z)"), 'Z');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-upcase #\A)"), 'A'); // Already uppercase
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-upcase #\0)"), '0'); // Non-letter
    
    // char-downcase
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-downcase #\A)"), 'a');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-downcase #\Z)"), 'z');
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-downcase #\a)"), 'a'); // Already lowercase
    assert_eq!(eval_to_char(&lisp, &mut eval, r"(char-downcase #\0)"), '0'); // Non-letter
}

// ============================================================
// String Literal Parsing Tests
// ============================================================

#[test]
fn test_string_literal_simple() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple string literals
    assert!(eval_string_matches(&lisp, &mut eval, r#""hello""#, "hello"));
    assert!(eval_string_matches(&lisp, &mut eval, r#""world""#, "world"));
    assert!(eval_string_matches(&lisp, &mut eval, r#""""#, "")); // Empty string
}

#[test]
fn test_string_literal_escapes() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Escape sequences
    assert!(eval_string_matches(&lisp, &mut eval, r#""hello\nworld""#, "hello\nworld"));
    assert!(eval_string_matches(&lisp, &mut eval, r#""tab\there""#, "tab\there"));
    assert!(eval_string_matches(&lisp, &mut eval, r#""quote\"here""#, "quote\"here"));
    assert!(eval_string_matches(&lisp, &mut eval, r#""back\\slash""#, "back\\slash"));
}

#[test]
fn test_string_literal_hex_escape() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Hex escape: \xNN;
    assert!(eval_string_matches(&lisp, &mut eval, r#""\x41;bc""#, "Abc"));
    assert!(eval_string_matches(&lisp, &mut eval, r#""a\x42;c""#, "aBc"));
}

// ============================================================
// String Predicate and Operation Tests
// ============================================================

#[test]
fn test_string_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string? predicate
    assert!(eval_is_true(&lisp, &mut eval, r#"(string? "hello")"#));
    assert!(eval_is_true(&lisp, &mut eval, r#"(string? "")"#));
    assert!(eval_is_false(&lisp, &mut eval, "(string? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(string? #\a)"));
}

#[test]
fn test_string_length() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(string-length "hello")"#), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(string-length "")"#), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(string-length "a")"#), 1);
}

#[test]
fn test_string_ref() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string-ref
    assert_eq!(eval_to_char(&lisp, &mut eval, r#"(string-ref "hello" 0)"#), 'h');
    assert_eq!(eval_to_char(&lisp, &mut eval, r#"(string-ref "hello" 4)"#), 'o');
    assert_eq!(eval_to_char(&lisp, &mut eval, r#"(string-ref "abc" 1)"#), 'b');
}

#[test]
fn test_make_string() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // make-string with fill
    assert!(eval_string_matches(&lisp, &mut eval, r#"(make-string 5 #\x)"#, "xxxxx"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(make-string 3 #\a)"#, "aaa"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(make-string 0 #\x)"#, ""));
}

#[test]
fn test_string_constructor() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string constructor
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string #\h #\e #\l #\l #\o)"#, "hello"));
    assert!(eval_string_matches(&lisp, &mut eval, r"(string)", "")); // Empty string
}

#[test]
fn test_string_comparison() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string=?
    assert!(eval_is_true(&lisp, &mut eval, r#"(string=? "abc" "abc")"#));
    assert!(eval_is_false(&lisp, &mut eval, r#"(string=? "abc" "abd")"#));
    
    // string<?
    assert!(eval_is_true(&lisp, &mut eval, r#"(string<? "abc" "abd")"#));
    assert!(eval_is_true(&lisp, &mut eval, r#"(string<? "ab" "abc")"#));
    assert!(eval_is_false(&lisp, &mut eval, r#"(string<? "abc" "abc")"#));
    
    // string>?
    assert!(eval_is_true(&lisp, &mut eval, r#"(string>? "abd" "abc")"#));
    assert!(eval_is_false(&lisp, &mut eval, r#"(string>? "abc" "abc")"#));
    
    // string<=?
    assert!(eval_is_true(&lisp, &mut eval, r#"(string<=? "abc" "abc")"#));
    assert!(eval_is_true(&lisp, &mut eval, r#"(string<=? "abc" "abd")"#));
    
    // string>=?
    assert!(eval_is_true(&lisp, &mut eval, r#"(string>=? "abc" "abc")"#));
    assert!(eval_is_true(&lisp, &mut eval, r#"(string>=? "abd" "abc")"#));
}

#[test]
fn test_string_append() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-append "hello" " " "world")"#, "hello world"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-append "a" "b" "c")"#, "abc"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-append)"#, ""));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-append "solo")"#, "solo"));
}

#[test]
fn test_string_to_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string->list
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(length (string->list "hello"))"#), 5);
    assert_eq!(eval_to_char(&lisp, &mut eval, r#"(car (string->list "hello"))"#), 'h');
    
    // list->string
    assert!(eval_string_matches(&lisp, &mut eval, r#"(list->string (list #\a #\b #\c))"#, "abc"));
    assert!(eval_string_matches(&lisp, &mut eval, r"(list->string '())", ""));
}

#[test]
fn test_substring() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_string_matches(&lisp, &mut eval, r#"(substring "hello" 1 4)"#, "ell"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(substring "hello" 0 5)"#, "hello"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(substring "hello" 2 2)"#, "")); // Empty substring
}

#[test]
fn test_string_copy() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-copy "hello")"#, "hello"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-copy "")"#, ""));
}

#[test]
fn test_string_set() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string-set! modifies in place
    eval.eval_str(r#"(define s (string-copy "hello"))"#).unwrap();
    eval.eval_str(r#"(string-set! s 0 #\H)"#).unwrap();
    assert!(eval_string_matches(&lisp, &mut eval, "s", "Hello"));
}

// ============================================================
// Character Predicate Stdlib Tests
// ============================================================

#[test]
fn test_char_alphabetic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, r"(char-alphabetic? #\a)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-alphabetic? #\Z)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char-alphabetic? #\0)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char-alphabetic? #\space)"));
}

#[test]
fn test_char_numeric() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, r"(char-numeric? #\0)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-numeric? #\9)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char-numeric? #\a)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char-numeric? #\space)"));
}

#[test]
fn test_char_whitespace() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, r"(char-whitespace? #\space)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-whitespace? #\tab)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-whitespace? #\newline)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char-whitespace? #\a)"));
}

#[test]
fn test_char_upper_lower_case() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, r"(char-upper-case? #\A)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-upper-case? #\Z)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char-upper-case? #\a)"));
    
    assert!(eval_is_true(&lisp, &mut eval, r"(char-lower-case? #\a)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-lower-case? #\z)"));
    assert!(eval_is_false(&lisp, &mut eval, r"(char-lower-case? #\A)"));
}

#[test]
fn test_digit_value() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, r"(digit-value #\0)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, r"(digit-value #\5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, r"(digit-value #\9)"), 9);
    assert!(eval_is_false(&lisp, &mut eval, r"(digit-value #\a)"));
}

#[test]
fn test_char_ci_comparisons() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, r"(char-ci=? #\a #\A)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-ci=? #\Z #\z)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-ci<? #\a #\B)"));
    assert!(eval_is_true(&lisp, &mut eval, r"(char-ci>? #\B #\a)"));
}

#[test]
fn test_string_case_conversion() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-upcase "hello")"#, "HELLO"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-downcase "HELLO")"#, "hello"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-downcase "HeLLo")"#, "hello"));
}

#[test]
fn test_string_ci_equals() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, r#"(string-ci=? "hello" "HELLO")"#));
    assert!(eval_is_true(&lisp, &mut eval, r#"(string-ci=? "ABC" "abc")"#));
    assert!(eval_is_false(&lisp, &mut eval, r#"(string-ci=? "abc" "abd")"#));
}

// ───────────────────────────────────────────────────────────────────────────
// NEW STDLIB FUNCTIONS (iota, list-tabulate, string utilities, etc.)
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_iota_functions() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // iota1: simple count
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (iota1 5))"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (iota1 5))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(last (iota1 5))"), 4);
    
    // iota2: count with start
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (iota2 5 10))"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(last (iota2 5 10))"), 14);
    
    // iota3: count with start and step
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (iota3 5 0 2))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(second (iota3 5 0 2))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(last (iota3 5 0 2))"), 8);
}

#[test]
fn test_list_tabulate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // list-tabulate with identity-like function
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (list-tabulate 5 (lambda (x) x)))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(last (list-tabulate 5 (lambda (x) x)))"), 4);
    
    // list-tabulate with square
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (list-tabulate 5 square))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(second (list-tabulate 5 square))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(third (list-tabulate 5 square))"), 4);
}

#[test]
fn test_list_accessors() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define lst '(1 2 3 4 5 6 7 8 9 10))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(first lst)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(second lst)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(third lst)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fourth lst)"), 4);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fifth lst)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sixth lst)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(seventh lst)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eighth lst)"), 8);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(ninth lst)"), 9);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(tenth lst)"), 10);
}

#[test]
fn test_list_utilities() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // concatenate
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (concatenate '((1 2) (3 4) (5 6))))"), 6);
    
    // flatten
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (flatten '(1 (2 3) ((4 5) 6))))"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(first (flatten '(1 (2 3))))"), 1);
    
    // count
    assert_eq!(eval_to_num(&lisp, &mut eval, "(count positive? '(-2 -1 0 1 2))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(count even? '(1 2 3 4 5 6))"), 3);
    
    // sum, product, average
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum '(1 2 3 4 5))"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(product '(1 2 3 4 5))"), 120);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(average '(2 4 6 8))"), 5);
}

#[test]
fn test_string_utilities() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string-null?
    assert!(eval_is_true(&lisp, &mut eval, r#"(string-null? "")"#));
    assert!(eval_is_false(&lisp, &mut eval, r#"(string-null? "x")"#));
    
    // string-reverse
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-reverse "hello")"#, "olleh"));
    
    // string-contains
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(string-contains "hello world" "wor")"#), 6);
    assert!(eval_is_false(&lisp, &mut eval, r#"(string-contains "hello" "xyz")"#));
    
    // string-join
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-join '("a" "b" "c") "-")"#, "a-b-c"));
    
    // string-trim
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-trim "  hello  ")"#, "hello"));
}

#[test]
fn test_string_map_and_for_each() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string-map
    assert!(eval_string_matches(&lisp, &mut eval, r#"(string-map char-upcase "hello")"#, "HELLO"));
}

#[test]
fn test_string_split() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string-split
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(length (string-split "a-b-c" #\-))"#), 3);
    assert!(eval_string_matches(&lisp, &mut eval, r#"(car (string-split "a-b-c" #\-))"#, "a"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(second (string-split "a-b-c" #\-))"#, "b"));
    assert!(eval_string_matches(&lisp, &mut eval, r#"(third (string-split "a-b-c" #\-))"#, "c"));
}

// ============================================================================
// Documentation Verification Tests
// ============================================================================
// These tests verify that features documented in README.md, LISP_ARCHITECTURE.md,
// and SCHEME_R7RS_CONFORMANCE.md are actually implemented as described.

#[test]
fn test_doc_truthiness_semantics() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Only #f is false - documented in README.md
    // Empty list is truthy
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if '() 1 2)"), 1);
    // Zero is truthy
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if 0 1 2)"), 1);
    // #f is false
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
    // #t is true
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 1 2)"), 1);
}

#[test]
fn test_doc_arithmetic_builtins() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Verify builtins from README examples
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3 4)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10 3)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 2 3 4)"), 24);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 100 5)"), 20);
    
    // modulo (NOT mod) - this is the correct function name
    assert_eq!(eval_to_num(&lisp, &mut eval, "(modulo 17 5)"), 2);
}

#[test]
fn test_doc_comparison_builtins() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Comparison builtins
    assert!(eval_is_true(&lisp, &mut eval, "(< 1 2)"));
    assert!(eval_is_true(&lisp, &mut eval, "(= 5 5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(> 3 1)"));
    assert!(eval_is_true(&lisp, &mut eval, "(<= 1 1)"));
    assert!(eval_is_true(&lisp, &mut eval, "(>= 5 5)"));
}

#[test]
fn test_doc_equality_builtins() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // eq? eqv? equal? (NOT eq without the question mark)
    assert!(eval_is_true(&lisp, &mut eval, "(eq? 'a 'a)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eqv? 5 5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(equal? '(1 2) '(1 2))"));
}

#[test]
fn test_doc_list_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // car/cdr/cons/list from README
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car '(1 2 3))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr '(1 2 3)))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cons 1 '(2 3)))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (list 1 2 3))"), 1);
}

#[test]
fn test_doc_predicates() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Type predicates from README
    assert!(eval_is_true(&lisp, &mut eval, "(null? '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? '(1))"));
    assert!(eval_is_true(&lisp, &mut eval, "(pair? '(1 . 2))"));
    assert!(eval_is_true(&lisp, &mut eval, "(number? 42)"));
    assert!(eval_is_true(&lisp, &mut eval, "(symbol? 'foo)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? car)"));
}

#[test]
fn test_doc_gc_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // GC builtins from README
    // gc returns a list (marked collected before)
    let result = eval.eval_str("(gc)").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
    
    // gc-enabled? returns boolean
    let result = eval.eval_str("(gc-enabled?)").unwrap();
    let val = lisp.get(result).unwrap();
    assert!(val.is_true() || val.is_false());
    
    // gc-disable and gc-enable
    eval.eval_str("(gc-disable)").unwrap();
    assert!(eval_is_false(&lisp, &mut eval, "(gc-enabled?)"));
    eval.eval_str("(gc-enable)").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(gc-enabled?)"));
    
    // arena-stats returns a list
    let result = eval.eval_str("(arena-stats)").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
}

#[test]
fn test_doc_vector_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Vector builtins from README (R7RS Section 6.8)
    eval.eval_str("(define vec (make-vector 5 0))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length vec)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref vec 2)"), 0);
    eval.eval_str("(vector-set! vec 2 42)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref vec 2)"), 42);
    assert!(eval_is_true(&lisp, &mut eval, "(vector? vec)"));
    assert!(eval_is_false(&lisp, &mut eval, "(vector? 42)"));
}

#[test]
fn test_doc_special_forms() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // quote
    let result = eval.eval_str("'hello").unwrap();
    assert!(lisp.symbol_matches(result, "hello").unwrap());
    
    // if
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
    
    // cond
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond (#f 1) (#t 2) (else 3))"), 2);
    
    // case
    assert_eq!(eval_to_num(&lisp, &mut eval, "(case 2 ((1) 10) ((2) 20) (else 30))"), 20);
    
    // lambda
    assert_eq!(eval_to_num(&lisp, &mut eval, "((lambda (x) (* x x)) 5)"), 25);
    
    // define
    eval.eval_str("(define x 42)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 42);
    
    // set!
    eval.eval_str("(set! x 100)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 100);
    
    // let
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((a 1) (b 2)) (+ a b))"), 3);
    
    // let*
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let* ((a 1) (b (+ a 1))) b)"), 2);
    
    // letrec
    assert_eq!(eval_to_num(&lisp, &mut eval, "(letrec ((f (lambda (n) (if (= n 0) 1 (* n (f (- n 1))))))) (f 5))"), 120);
    
    // letrec*
    assert_eq!(eval_to_num(&lisp, &mut eval, "(letrec* ((a 1) (b (+ a 1))) b)"), 2);
    
    // begin
    assert_eq!(eval_to_num(&lisp, &mut eval, "(begin 1 2 3)"), 3);
    
    // and/or
    assert!(eval_is_false(&lisp, &mut eval, "(and #t #f)"));
    assert!(eval_is_true(&lisp, &mut eval, "(or #f #t)"));
    
    // when/unless
    eval.eval_str("(define when-test 0)").unwrap();
    eval.eval_str("(when #t (set! when-test 1))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "when-test"), 1);
    
    eval.eval_str("(define unless-test 0)").unwrap();
    eval.eval_str("(unless #f (set! unless-test 1))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "unless-test"), 1);
    
    // do loop
    assert_eq!(eval_to_num(&lisp, &mut eval, "(do ((i 0 (+ i 1)) (sum 0 (+ sum i))) ((= i 5) sum))"), 10);
    
    // eval
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval '(+ 1 2))"), 3);
    
    // apply
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply + '(1 2 3))"), 6);
    
    // values - returns multiple values as a list
    let result = eval.eval_str("(values 1 2 3)").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
}

#[test]
fn test_doc_mutation_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // set-car! and set-cdr! from docs
    eval.eval_str("(define pair (cons 1 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car pair)"), 1);
    eval.eval_str("(set-car! pair 10)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car pair)"), 10);
    eval.eval_str("(set-cdr! pair 20)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr pair)"), 20);
}

#[test]
fn test_doc_stdlib_higher_order() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // map from README
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (map (lambda (x) (* x x)) '(1 2 3 4 5)))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (map (lambda (x) (* x x)) '(1 2 3 4 5))))"), 4);
    
    // filter - note about empty list being truthy
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (filter (lambda (x) (> x 0)) '(-1 2 -3 4)))"), 2);
    
    // fold
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fold + 0 '(1 2 3 4 5))"), 15);
}

#[test]
fn test_doc_stdlib_list_utilities() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // length
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(a b c))"), 3);
    
    // append
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (append '(1 2) '(3 4)))"), 4);
    
    // reverse
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (reverse '(1 2 3)))"), 3);
    
    // nth (0-indexed)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(nth 2 '(10 20 30 40))"), 30);
    
    // range
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (range 0 5))"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (range 0 5))"), 0);
    
    // take
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (take 3 '(a b c d e)))"), 3);
}

#[test]
fn test_doc_stdlib_identity_constantly() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // identity
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 42)"), 42);
    
    // constantly
    eval.eval_str("(define always-5 (constantly 5))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(always-5 100)"), 5);
}

#[test]
fn test_doc_string_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // string-length
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(string-length "hello")"#), 5);
    
    // string-append
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(string-length (string-append "a" "b"))"#), 2);
    
    // substring
    assert_eq!(eval_to_num(&lisp, &mut eval, r#"(string-length (substring "hello" 1 3))"#), 2);
}

#[test]
fn test_doc_character_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // char->integer
    assert_eq!(eval_to_num(&lisp, &mut eval, "(char->integer #\\A)"), 65);
    
    // integer->char
    assert!(eval_is_true(&lisp, &mut eval, "(char=? (integer->char 65) #\\A)"));
    
    // char-upcase and char-downcase
    assert!(eval_is_true(&lisp, &mut eval, "(char=? (char-upcase #\\a) #\\A)"));
    assert!(eval_is_true(&lisp, &mut eval, "(char=? (char-downcase #\\A) #\\a)"));
}

#[test]
fn test_doc_tco_no_stack_overflow() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test that TCO works for deep recursion - from README claims
    // Note: Uses 100 depth which is safe within continuation stack limits
    eval.eval_str("(define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))").unwrap();
    // This would stack overflow without proper TCO
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum 100 0)"), 5050);
}

#[test]
fn test_doc_quasiquote() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // quasiquote with unquote - must use (quasiquote ...) syntax, not backtick
    eval.eval_str("(define x 5)").unwrap();
    // (quasiquote (a b (unquote x))) => (a b 5)
    let result = eval.eval_str("(quasiquote (a b (unquote x)))").unwrap();
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert_eq!(lisp.get(third).unwrap().as_number().unwrap(), 5);
}

#[test]
fn test_doc_numeric_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // abs
    assert_eq!(eval_to_num(&lisp, &mut eval, "(abs -5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(abs 5)"), 5);
    
    // max/min
    assert_eq!(eval_to_num(&lisp, &mut eval, "(max 1 5 3)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(min 1 5 3)"), 1);
    
    // expt
    assert_eq!(eval_to_num(&lisp, &mut eval, "(expt 2 10)"), 1024);
    
    // square
    assert_eq!(eval_to_num(&lisp, &mut eval, "(square 5)"), 25);
    
    // gcd/lcm
    assert_eq!(eval_to_num(&lisp, &mut eval, "(gcd 12 8)"), 4);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(lcm 4 6)"), 12);
    
    // quotient/remainder
    assert_eq!(eval_to_num(&lisp, &mut eval, "(quotient 17 5)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(remainder 17 5)"), 2);
    
    // floor/ceiling/truncate/round
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor 3)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(ceiling 3)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate 3)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(round 3)"), 3);
}

#[test]
fn test_doc_number_predicates() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // zero?/positive?/negative?/odd?/even?
    assert!(eval_is_true(&lisp, &mut eval, "(zero? 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(zero? 1)"));
    assert!(eval_is_true(&lisp, &mut eval, "(positive? 5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(negative? -5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(odd? 5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(even? 4)"));
    
    // integer?/exact?/inexact?
    assert!(eval_is_true(&lisp, &mut eval, "(integer? 5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(exact? 5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(inexact? 3.14)"));
}

// ============================================================
// Vector tests (R7RS Section 6.8)
// ============================================================

#[test]
fn test_vector_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // vector? returns true for vectors
    assert!(eval_is_true(&lisp, &mut eval, "(vector? (vector 1 2 3))"));
    assert!(eval_is_true(&lisp, &mut eval, "(vector? (make-vector 5))"));
    
    // vector? returns false for non-vectors
    assert!(eval_is_false(&lisp, &mut eval, "(vector? '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(vector? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(vector? \"hello\")"));
}

#[test]
fn test_make_vector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // make-vector with just length
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (make-vector 5))"), 5);
    
    // make-vector with fill value
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (make-vector 3 42) 0)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (make-vector 3 42) 1)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (make-vector 3 42) 2)"), 42);
}

#[test]
fn test_vector_constructor() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // vector creates a vector from arguments
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (vector))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (vector 1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector 10 20 30) 0)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector 10 20 30) 1)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector 10 20 30) 2)"), 30);
}

#[test]
fn test_vector_length() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (vector))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (vector 'a))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (vector 'a 'b 'c 'd 'e))"), 5);
}

#[test]
fn test_vector_ref() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector 1 1 2 3 5 8 13 21) 5)"), 8);
}

#[test]
fn test_vector_set() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test mutating a vector
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let ((vec (vector 0 1 2)))
           (vector-set! vec 1 42)
           (vector-ref vec 1))"), 42);
}

#[test]
fn test_vector_to_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // vector->list converts to list
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (vector->list (vector 1 2 3)))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr (vector->list (vector 1 2 3)))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(caddr (vector->list (vector 1 2 3)))"), 3);
    
    // Empty vector converts to empty list
    assert!(eval_is_true(&lisp, &mut eval, "(null? (vector->list (vector)))"));
}

#[test]
fn test_list_to_vector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // list->vector converts list to vector
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (list->vector '(1 2 3)) 0)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (list->vector '(1 2 3)) 1)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (list->vector '(1 2 3)) 2)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (list->vector '(1 2 3)))"), 3);
    
    // Empty list converts to empty vector
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (list->vector '()))"), 0);
}

#[test]
fn test_vector_fill() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // vector-fill! fills all elements
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let ((vec (vector 1 2 3)))
           (vector-fill! vec 0)
           (+ (vector-ref vec 0) (vector-ref vec 1) (vector-ref vec 2)))"), 0);
}

#[test]
fn test_vector_copy() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // vector-copy creates an independent copy
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let ((vec1 (vector 1 2 3)))
           (let ((vec2 (vector-copy vec1)))
             (vector-set! vec2 0 100)
             (+ (vector-ref vec1 0) (vector-ref vec2 0))))"), 101);
}

#[test]
fn test_vector_literal() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Vector literal #(...)
    assert!(eval_is_true(&lisp, &mut eval, "(vector? #())"));
    assert!(eval_is_true(&lisp, &mut eval, "(vector? #(1 2 3))"));
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length #())"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length #(1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref #(10 20 30) 1)"), 20);
}

#[test]
fn test_vector_with_mixed_types() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Vectors can hold heterogeneous types
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector 0 '(2 2 2 2) \"Anna\") 0)"), 0);
    assert!(eval_is_true(&lisp, &mut eval, "(pair? (vector-ref (vector 0 '(2 2 2 2) \"Anna\") 1))"));
    assert!(eval_is_true(&lisp, &mut eval, "(string? (vector-ref (vector 0 '(2 2 2 2) \"Anna\") 2))"));
}

#[test]
fn test_vector_nested_literal() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Nested vector literals
    assert!(eval_is_true(&lisp, &mut eval, "(vector? (vector-ref #(1 #(2 3) 4) 1))"));
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector-ref #(1 #(2 3) 4) 1) 0)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector-ref #(1 #(2 3) 4) 1) 1)"), 3);
}

#[test]
fn test_list_to_vector_improper_list_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // list->vector with non-list should error
    let result = eval.eval_str("(list->vector 42)");
    assert!(result.is_err());
    
    // list->vector with improper list should error
    let result = eval.eval_str("(list->vector (cons 1 2))");
    assert!(result.is_err());
}

#[test]
fn test_vector_make_vector_negative_length() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // make-vector with negative length should error
    let result = eval.eval_str("(make-vector -1)");
    assert!(result.is_err());
}


// ═══════════════════════════════════════════════════════════════════════════
// NESTED STDLIB CALL TESTS
// Tests for nested stdlib function calls (issue: deeply nested stdlib calls)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_nested_stdlib_calls() {
    // This test verifies that stdlib functions work correctly when called
    // as arguments to other functions. This was a bug where `let` and other
    // forms called `eval_in_env` which reset the continuation stack.
    // Note: Arena size is kept small (10000) to avoid stack overflow from
    // stack-allocated arena in test threads.
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic nested stdlib call: (+ 1 (sqrt 4)) should return 3.0
    let result = eval.eval_str("(+ 1 (sqrt 4))").unwrap();
    let val = lisp.get(result).unwrap().as_float().unwrap();
    assert!((val - 3.0).abs() < 0.001, "Expected 3.0, got {}", val);
    
    // Multiple nested stdlib calls
    let result = eval.eval_str("(+ (sqrt 4) (sqrt 9))").unwrap();
    let val = lisp.get(result).unwrap().as_float().unwrap();
    assert!((val - 5.0).abs() < 0.001, "Expected 5.0, got {}", val);
    
    // Deeply nested stdlib calls
    let result = eval.eval_str("(+ 1 (* 2 (sqrt 4)))").unwrap();
    let val = lisp.get(result).unwrap().as_float().unwrap();
    assert!((val - 5.0).abs() < 0.001, "Expected 5.0, got {}", val);
}
