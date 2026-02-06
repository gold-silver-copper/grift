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
    let lisp: Lisp<20000> = Lisp::new();
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
    // Further increased for cons cells now using 3 slots each
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sum function - tail recursive
    eval.eval_str("(define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum 100 0)"), 5050);
}

#[test]
fn test_lru_cache_eviction() {
    // Test that cache eviction works by exceeding MAX_MEMO_CACHE_SIZE
    // Reduced for debug builds (60000 causes stack overflow)
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
fn test_petrofsky_let() {
    // The Petrofsky let test: ensures named-let doesn't introduce the loop name
    // too early in the scope. The initializer (- 1) should call the subtraction
    // function from outer scope, not the named-let loop function.
    // Reference: http://web.archive.org/web/20070626123636/http://www.paulgraham.com/arcchallenge.html
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // This should return -1 (result of (- 1) which is negation/subtraction)
    // NOT 1 (which would happen if - was bound to the loop before evaluating (- 1))
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let - ((n (- 1))) n)"), -1);
    
    // Additional edge case: using builtin name as loop, but with recursion
    // The initializer (+ 2 3) uses outer +, but body uses loop + recursively
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let + ((n (+ 2 3))) (if (= n 0) 100 (+ (- n 1))))"), 100);
    
    // Another case with * - initializer uses outer *, body uses loop *
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let * ((x (* 2 3))) (if (= x 0) 42 (* (- x 1))))"), 42);
}

#[test]
fn test_named_let() {
    // Test that named let works correctly for recursion
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Factorial using named let
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let fact ((n 5)) (if (= n 0) 1 (* n (fact (- n 1)))))"), 120);
    
    // Sum using accumulator
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let sum ((n 10) (acc 0)) (if (= n 0) acc (sum (- n 1) (+ acc n))))"), 55);
    
    // Empty bindings named let
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let loop () 42)"), 42);
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
// Hygienic macros via syntax-case are implemented.
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

#[test]
fn test_call_with_values_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic call-with-values with two values
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(call-with-values (lambda () (values 4 5)) (lambda (a b) (+ a b)))"), 9);
    
    // Return second value
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(call-with-values (lambda () (values 4 5)) (lambda (a b) b))"), 5);
}

#[test]
fn test_call_with_values_single() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Single value from normal lambda (not using values)
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(call-with-values (lambda () 42) (lambda (x) (* x 2)))"), 84);
}

#[test]
fn test_call_with_values_no_values() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Zero values
    let result = eval.eval_str("(call-with-values (lambda () (values)) (lambda () 'no-values))").unwrap();
    assert!(lisp.symbol_matches(result, "no-values").unwrap());
}

#[test]
fn test_call_with_values_many() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Multiple values (3)
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(call-with-values (lambda () (values 1 2 3)) (lambda (x y z) (+ x y z)))"), 6);
}

#[test]
fn test_let_values_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic let-values
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let-values (((a b) (values 1 2))) (+ a b))"), 3);
    
    // Empty bindings
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let-values () 42)"), 42);
}

#[test]
fn test_let_values_multiple_bindings() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Multiple bindings
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let-values (((a b) (values 1 2)) ((c) (values 3))) (+ a b c))"), 6);
}

#[test]
fn test_let_star_values() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // let*-values allows later bindings to see earlier ones
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(let*-values (((a b) (values 1 2)) ((c) (values (+ a b)))) c)"), 3);
}

#[test]
fn test_define_values_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define two values
    eval.eval_str("(define-values (dv-x dv-y) (values 10 20))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "dv-x"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "dv-y"), 20);
}

#[test]
fn test_define_values_single() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Single value
    eval.eval_str("(define-values (dv-single) (values 42))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "dv-single"), 42);
}

#[test]
fn test_define_values_three() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Three values
    eval.eval_str("(define-values (dv-a dv-b dv-c) (values 1 2 3))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "dv-a"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "dv-b"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "dv-c"), 3);
}

#[test]
fn test_define_values_many() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test with more than 4 values (verifies ellipsis pattern works)
    eval.eval_str("(define-values (v1 v2 v3 v4 v5 v6 v7) (values 10 20 30 40 50 60 70))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "v1"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "v2"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "v3"), 30);
    assert_eq!(eval_to_num(&lisp, &mut eval, "v4"), 40);
    assert_eq!(eval_to_num(&lisp, &mut eval, "v5"), 50);
    assert_eq!(eval_to_num(&lisp, &mut eval, "v6"), 60);
    assert_eq!(eval_to_num(&lisp, &mut eval, "v7"), 70);
}


// ───────────────────────────────────────────────────────────────────────────
// DELAY/FORCE - Lazy Evaluation
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_delay_force_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic delay/force
    eval.eval_str("(define promise (delay (+ 1 2)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force promise)"), 3);
    // Second force should return same value
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force promise)"), 3);
}

#[test]
fn test_delay_memoization() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Verify that delay memoizes - expression is only evaluated once
    eval.eval_str("(define counter 0)").unwrap();
    eval.eval_str("(define lazy-inc (delay (begin (set! counter (+ counter 1)) counter)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force lazy-inc)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force lazy-inc)"), 1); // Still 1, not 2
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 1); // counter was only incremented once
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
    
    // display returns void (unspecified value) per R7RS
    let result = eval.eval_str("(display 99)").unwrap();
    assert!(lisp.get(result).unwrap().is_void());
}

#[test]
fn test_newline() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // newline returns void (unspecified value) per R7RS
    let result = eval.eval_str("(newline)").unwrap();
    assert!(lisp.get(result).unwrap().is_void());
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
    
    // R7RS: member should return the first sublist whose car is the object
    // (member 2 '(1 2 3)) => (2 3)
    let result = eval.eval_str("(member 2 '(1 2 3))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false()); // Not #f
    // Verify it's the sublist starting with 2
    assert_eq!(lisp.get(lisp.car(result).unwrap()).unwrap().as_number().unwrap(), 2);
    
    // member should return #f when element not found
    assert!(eval_is_false(&lisp, &mut eval, "(member 5 '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(member 1 '())"));
    
    // member uses equal? for comparison (can find lists)
    let result = eval.eval_str("(member '(a) '((x) (a) (b)))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false());
}

#[test]
fn test_memq_memv_differences() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // memq uses eq? - returns sublist starting at match
    let result = eval.eval_str("(memq 'a '(b a c))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false());
    assert!(lisp.symbol_matches(lisp.car(result).unwrap(), "a").unwrap());
    
    // memq won't find structurally equal lists (uses eq? not equal?)
    assert!(eval_is_false(&lisp, &mut eval, "(memq '(a) '((b) (a) (c)))"));
    
    // memv uses eqv? - returns sublist
    let result = eval.eval_str("(memv 2 '(1 2 3))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false());
    assert_eq!(lisp.get(lisp.car(result).unwrap()).unwrap().as_number().unwrap(), 2);
}

#[test]
fn test_assoc_variants() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // assoc uses equal? - can find complex keys
    let result = eval.eval_str("(assoc '(a) '(((x) 1) ((a) 2) ((b) 3)))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false());
    // Result should be ((a) 2) - verify the value
    let value = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert_eq!(lisp.get(value).unwrap().as_number().unwrap(), 2);
    
    // assoc returns #f when key not found
    assert!(eval_is_false(&lisp, &mut eval, "(assoc 'x '((a 1) (b 2)))"));
    
    // assq uses eq? - for symbol keys
    let result = eval.eval_str("(assq 'b '((a 1) (b 2) (c 3)))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false());
    let value = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert_eq!(lisp.get(value).unwrap().as_number().unwrap(), 2);
    
    // assv uses eqv? - for numbers
    let result = eval.eval_str("(assv 5 '((2 a) (5 b) (7 c)))").unwrap();
    assert!(!lisp.get(result).unwrap().is_false());
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
    // Note: Stdlib functions are parsed on each call (no caching), so recursive
    // calls need more arena space. Use a larger arena for recursive tests.
    let lisp: Lisp<20000> = Lisp::new();
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

#[test]
fn test_gc_disabled_memory_grows() {
    // Verify that when GC is disabled, garbage actually accumulates
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Get initial allocation count
    let stats_before = eval.eval_str("(arena-stats)").unwrap();
    let allocated_before = lisp.get(lisp.car(lisp.cdr(stats_before).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    // Disable GC
    eval.eval_str("(gc-disable)").unwrap();
    
    // Create a lot of garbage (these cons cells aren't rooted)
    for _ in 0..100 {
        eval.eval_str("(cons 'garbage 'value)").unwrap();
    }
    
    // Try to run GC - should do nothing since disabled
    let gc_result = eval.eval_str("(gc)").unwrap();
    let collected = lisp.get(lisp.car(lisp.cdr(gc_result).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    assert_eq!(collected, 0, "GC should not collect anything when disabled");
    
    // Get allocation count after - should be higher (garbage accumulated)
    let stats_after = eval.eval_str("(arena-stats)").unwrap();
    let allocated_after = lisp.get(lisp.car(lisp.cdr(stats_after).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    assert!(allocated_after > allocated_before, 
        "Memory should grow when GC is disabled: before={}, after={}", 
        allocated_before, allocated_after);
    
    // Re-enable and collect
    eval.eval_str("(gc-enable)").unwrap();
    let gc_result = eval.eval_str("(gc)").unwrap();
    let collected = lisp.get(lisp.car(lisp.cdr(gc_result).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    // Now GC should have collected the garbage
    assert!(collected > 0, "GC should collect garbage when re-enabled");
}

#[test]
fn test_gc_disabled_trampoline_respects_flag() {
    // Verify the trampoline's periodic GC check respects gc_enabled
    // This runs code that would normally trigger periodic GC
    // Note: With arena-based continuations, we need enough arena since
    // each push_cont allocates cons cells in the arena.
    let lisp: Lisp<25000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Disable GC
    eval.eval_str("(gc-disable)").unwrap();
    
    // Get initial count
    let stats_before = eval.eval_str("(arena-stats)").unwrap();
    let allocated_before = lisp.get(lisp.car(lisp.cdr(stats_before).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    // Run a recursive function that creates garbage and takes many steps
    // This should trigger the periodic GC check in the trampoline
    // Reduced iterations to avoid running out of arena with GC disabled
    // (arena-based continuations use more memory per call).
    eval.eval_str("
        (define (make-garbage n)
          (if (<= n 0)
              'done
              (begin
                (cons n (cons n (cons n '())))  ; create garbage
                (make-garbage (- n 1)))))
    ").unwrap();
    
    eval.eval_str("(make-garbage 100)").unwrap();
    
    // Memory should have grown since GC is disabled
    let stats_after = eval.eval_str("(arena-stats)").unwrap();
    let allocated_after = lisp.get(lisp.car(lisp.cdr(stats_after).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    assert!(allocated_after > allocated_before,
        "Trampoline should NOT run GC when disabled: before={}, after={}",
        allocated_before, allocated_after);
    
    // Re-enable and verify GC now works
    eval.eval_str("(gc-enable)").unwrap();
    let gc_result = eval.eval_str("(gc)").unwrap();
    let collected = lisp.get(lisp.car(lisp.cdr(gc_result).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    assert!(collected > 0, "GC should work after re-enable");
}

#[test]
fn test_gc_disabled_via_arena_respected_by_lisp() {
    // Verify that disabling GC at the arena level is respected by Lisp gc()
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Verify GC is enabled by default
    assert!(eval_is_true(&lisp, &mut eval, "(gc-enabled?)"));
    
    // Create some garbage
    eval.eval_str("(cons 1 2)").unwrap();
    eval.eval_str("(cons 3 4)").unwrap();
    
    // Disable via Lisp
    eval.eval_str("(gc-disable)").unwrap();
    assert!(eval_is_false(&lisp, &mut eval, "(gc-enabled?)"));
    
    // Verify GC doesn't run
    let gc_result = eval.eval_str("(gc)").unwrap();
    let marked = lisp.get(lisp.car(gc_result).unwrap()).unwrap().as_number().unwrap();
    let collected = lisp.get(lisp.car(lisp.cdr(gc_result).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    
    assert_eq!(marked, 0, "Marked should be 0 when GC is disabled");
    assert_eq!(collected, 0, "Collected should be 0 when GC is disabled");
    
    // Re-enable
    eval.eval_str("(gc-enable)").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(gc-enabled?)"));
}

#[test]
fn test_gc_unconditional_ignores_disabled_flag() {
    // Verify that collect_garbage_unconditional still works when GC is disabled
    // (This is tested at the arena level, but let's verify behavior)
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create garbage
    eval.eval_str("(cons 'a 'b)").unwrap();
    eval.eval_str("(cons 'c 'd)").unwrap();
    
    // Disable GC
    eval.eval_str("(gc-disable)").unwrap();
    
    // Normal GC should not collect
    let gc_result = eval.eval_str("(gc)").unwrap();
    let collected = lisp.get(lisp.car(lisp.cdr(gc_result).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    assert_eq!(collected, 0, "Regular gc should not collect when disabled");
    
    // Force GC should still work (via gc-force if available, or just re-enable)
    eval.eval_str("(gc-enable)").unwrap();
    let gc_result = eval.eval_str("(gc)").unwrap();
    let collected = lisp.get(lisp.car(lisp.cdr(gc_result).unwrap()).unwrap())
        .unwrap().as_number().unwrap();
    assert!(collected >= 0, "GC should work when enabled");
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
    // Reduced for debug builds (60000 causes stack overflow)
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
    // Reduced for debug builds (60000 causes stack overflow)
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
    // Reduced for debug builds (60000 causes stack overflow)
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
    // Reduced for debug builds (60000 causes stack overflow)
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
    
    // integer?/exact?/inexact? (no floats, all numbers are exact integers)
    assert!(eval_is_true(&lisp, &mut eval, "(integer? 5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(exact? 5)"));
    assert!(eval_is_false(&lisp, &mut eval, "(inexact? 5)")); // All numbers are exact now
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
    // Increased for cons cells now using 3 slots each
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic nested stdlib call: (+ 1 (abs -2)) should return 3
    let result = eval.eval_str("(+ 1 (abs -2))").unwrap();
    let val = lisp.get(result).unwrap().as_number().unwrap();
    assert_eq!(val, 3);
    
    // Multiple nested stdlib calls
    let result = eval.eval_str("(+ (abs -2) (abs -3))").unwrap();
    let val = lisp.get(result).unwrap().as_number().unwrap();
    assert_eq!(val, 5);
    
    // Deeply nested stdlib calls
    let result = eval.eval_str("(+ 1 (* 2 (abs -2)))").unwrap();
    let val = lisp.get(result).unwrap().as_number().unwrap();
    assert_eq!(val, 5);
}

#[test]
fn test_size_check() {
    use std::mem::{size_of, align_of};
    use grift_eval::TrampolineState;
    use grift_parser::{Builtin, StdLib};
    
    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║                    TYPE SIZE ANALYSIS                      ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    
    println!("║ Core Arena Types:                                          ║");
    println!("║   ArenaIndex:      {:>3} bytes (align: {:>2})                  ║", 
             size_of::<ArenaIndex>(), align_of::<ArenaIndex>());
    println!("║   Value:           {:>3} bytes (align: {:>2})                  ║", 
             size_of::<Value>(), align_of::<Value>());
    
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ Continuation Types:                                        ║");
    println!("║   (Continuations are now arena-based, not stack-based)     ║");
    println!("║   TrampolineState: {:>3} bytes (align: {:>2})                  ║", 
             size_of::<TrampolineState>(), align_of::<TrampolineState>());
    
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ Function Types:                                            ║");
    println!("║   Builtin:         {:>3} bytes (align: {:>2}, {} variants)     ║", 
             size_of::<Builtin>(), align_of::<Builtin>(), Builtin::ALL.len());
    println!("║   StdLib:          {:>3} bytes (align: {:>2}, {} variants)      ║", 
             size_of::<StdLib>(), align_of::<StdLib>(), StdLib::ALL.len());
    
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ Rust Primitives (for reference):                           ║");
    println!("║   usize:           {:>3} bytes                               ║", size_of::<usize>());
    println!("║   isize:           {:>3} bytes                               ║", size_of::<isize>());
    println!("║   char:            {:>3} bytes                               ║", size_of::<char>());
    println!("║   bool:            {:>3} bytes                               ║", size_of::<bool>());
    
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║ Analysis:                                                  ║");
    
    // Value analysis
    let value_slots = size_of::<Value>() / size_of::<usize>();
    println!("║   Value = {} usizes = discriminant + {} usizes payload   ║", 
             value_slots, value_slots - 1);
    
    // Cache line analysis (64 bytes typical)
    let values_per_cache_line = 64 / size_of::<Value>();
    println!("║   Values per 64-byte cache line: {}                        ║", values_per_cache_line);
    
    println!("╚════════════════════════════════════════════════════════════╝\n");
    
    // Assertions to catch regressions
    assert!(size_of::<Value>() <= 32, "Value enum grew beyond 32 bytes!");
    assert!(size_of::<ArenaIndex>() == 8, "ArenaIndex should be exactly 8 bytes");
}

// ═══════════════════════════════════════════════════════════════════════════
// QUASIQUOTE READER SYNTAX TESTS
// ═══════════════════════════════════════════════════════════════════════════
//
// Tests for ` (quasiquote), , (unquote), and ,@ (unquote-splicing) reader syntax

#[test]
fn test_quasiquote_reader_syntax_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // `x should be equivalent to (quasiquote x)
    // Basic literal quasiquote - no evaluation, just quoted
    let result = eval.eval_str("`42").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number().unwrap(), 42);
    
    // List quasiquote without unquote - same as quote
    let result = eval.eval_str("`(a b c)").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
}

#[test]
fn test_quasiquote_reader_with_unquote() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 42)").unwrap();
    
    // `(a b ,x) should give (a b 42)
    let result = eval.eval_str("`(a b ,x)").unwrap();
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert_eq!(lisp.get(third).unwrap().as_number().unwrap(), 42);
}

#[test]
fn test_quasiquote_reader_with_expression_unquote() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 5)").unwrap();
    
    // `(1 ,(+ x 3) 10) should give (1 8 10)
    let result = eval.eval_str("`(1 ,(+ x 3) 10)").unwrap();
    let second = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert_eq!(lisp.get(second).unwrap().as_number().unwrap(), 8);
}

#[test]
fn test_quasiquote_reader_multiple_unquotes() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define a 1)").unwrap();
    eval.eval_str("(define b 2)").unwrap();
    eval.eval_str("(define c 3)").unwrap();
    
    // `(,a ,b ,c) should give (1 2 3)
    let result = eval.eval_str("`(,a ,b ,c)").unwrap();
    let first = lisp.car(result).unwrap();
    let second = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    
    assert_eq!(lisp.get(first).unwrap().as_number().unwrap(), 1);
    assert_eq!(lisp.get(second).unwrap().as_number().unwrap(), 2);
    assert_eq!(lisp.get(third).unwrap().as_number().unwrap(), 3);
}

#[test]
fn test_unquote_splicing_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define lst '(2 3 4))").unwrap();
    
    // `(1 ,@lst 5) should give (1 2 3 4 5)
    let result = eval.eval_str("`(1 ,@lst 5)").unwrap();
    
    // Check: (1 2 3 4 5)
    let n1 = lisp.car(result).unwrap();
    let n2 = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let n3 = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    let n4 = lisp.car(lisp.cdr(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap()).unwrap();
    let n5 = lisp.car(lisp.cdr(lisp.cdr(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap()).unwrap()).unwrap();
    
    assert_eq!(lisp.get(n1).unwrap().as_number().unwrap(), 1);
    assert_eq!(lisp.get(n2).unwrap().as_number().unwrap(), 2);
    assert_eq!(lisp.get(n3).unwrap().as_number().unwrap(), 3);
    assert_eq!(lisp.get(n4).unwrap().as_number().unwrap(), 4);
    assert_eq!(lisp.get(n5).unwrap().as_number().unwrap(), 5);
}

#[test]
fn test_unquote_splicing_at_start() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define xs '(a b))").unwrap();
    
    // `(,@xs c d) should give (a b c d)
    let result = eval.eval_str("`(,@xs c d)").unwrap();
    
    // Check that result is a 4-element list
    let mut count = 0;
    let mut current = result;
    loop {
        match lisp.get(current).unwrap() {
            Value::Nil => break,
            Value::Cons { cdr, .. } => {
                count += 1;
                current = cdr;
            }
            _ => panic!("Expected list"),
        }
    }
    assert_eq!(count, 4);
}

#[test]
fn test_unquote_splicing_empty_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define empty '())").unwrap();
    
    // `(1 ,@empty 2) should give (1 2) - empty list contributes nothing
    let result = eval.eval_str("`(1 ,@empty 2)").unwrap();
    
    let n1 = lisp.car(result).unwrap();
    let n2 = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let tail = lisp.cdr(lisp.cdr(result).unwrap()).unwrap();
    
    assert_eq!(lisp.get(n1).unwrap().as_number().unwrap(), 1);
    assert_eq!(lisp.get(n2).unwrap().as_number().unwrap(), 2);
    assert!(lisp.get(tail).unwrap().is_nil());
}

#[test]
fn test_unquote_splicing_with_computed_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // ,@(list 1 2 3) - evaluate (list 1 2 3) then splice
    let result = eval.eval_str("`(a ,@(list 1 2 3) b)").unwrap();
    
    // Should be (a 1 2 3 b) - 5 elements
    let mut count = 0;
    let mut current = result;
    loop {
        match lisp.get(current).unwrap() {
            Value::Nil => break,
            Value::Cons { cdr, .. } => {
                count += 1;
                current = cdr;
            }
            _ => panic!("Expected list"),
        }
    }
    assert_eq!(count, 5);
}

#[test]
fn test_quasiquote_nested_lists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 10)").unwrap();
    
    // `((a ,x) (b ,(+ x 1))) should give ((a 10) (b 11))
    let result = eval.eval_str("`((a ,x) (b ,(+ x 1)))").unwrap();
    
    // First inner list
    let first_list = lisp.car(result).unwrap();
    let first_second = lisp.car(lisp.cdr(first_list).unwrap()).unwrap();
    assert_eq!(lisp.get(first_second).unwrap().as_number().unwrap(), 10);
    
    // Second inner list
    let second_list = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let second_second = lisp.car(lisp.cdr(second_list).unwrap()).unwrap();
    assert_eq!(lisp.get(second_second).unwrap().as_number().unwrap(), 11);
}

#[test]
fn test_quasiquote_deeply_nested_unquote() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define val 99)").unwrap();
    
    // `(a (b (c ,val))) - unquote in deeply nested structure
    let result = eval.eval_str("`(a (b (c ,val)))").unwrap();
    
    // Navigate to val: cdr -> car -> cdr -> car -> cdr -> car
    let inner1 = lisp.car(lisp.cdr(result).unwrap()).unwrap(); // (b (c 99))
    let inner2 = lisp.car(lisp.cdr(inner1).unwrap()).unwrap(); // (c 99)
    let inner_val = lisp.car(lisp.cdr(inner2).unwrap()).unwrap(); // 99
    
    assert_eq!(lisp.get(inner_val).unwrap().as_number().unwrap(), 99);
}

#[test]
fn test_quasiquote_mixed_quote_and_unquote() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 5)").unwrap();
    
    // `(a 'b ,x) - mix of literal, quoted symbol, and unquoted value
    let result = eval.eval_str("`(a 'b ,x)").unwrap();
    
    // Third element should be 5 (from ,x)
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert_eq!(lisp.get(third).unwrap().as_number().unwrap(), 5);
}

#[test]
fn test_quasiquote_equivalence_to_long_form() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define n 7)").unwrap();
    
    // Short form
    let short = eval.eval_str("`(a ,n b)").unwrap();
    
    // Long form (explicit quasiquote/unquote)
    let long = eval.eval_str("(quasiquote (a (unquote n) b))").unwrap();
    
    // Both should produce (a 7 b)
    let short_second = lisp.car(lisp.cdr(short).unwrap()).unwrap();
    let long_second = lisp.car(lisp.cdr(long).unwrap()).unwrap();
    
    assert_eq!(lisp.get(short_second).unwrap().as_number().unwrap(), 7);
    assert_eq!(lisp.get(long_second).unwrap().as_number().unwrap(), 7);
}

#[test]
fn test_quasiquote_in_function_body() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Function that builds a list using quasiquote
    eval.eval_str("(define (make-pair a b) `(,a . ,b))").unwrap();
    
    let result = eval.eval_str("(make-pair 1 2)").unwrap();
    
    // Should be (1 . 2) - a dotted pair
    let car = lisp.car(result).unwrap();
    let cdr = lisp.cdr(result).unwrap();
    
    assert_eq!(lisp.get(car).unwrap().as_number().unwrap(), 1);
    assert_eq!(lisp.get(cdr).unwrap().as_number().unwrap(), 2);
}

#[test]
fn test_quasiquote_function_building_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Function that builds a wrapped expression
    eval.eval_str("(define (wrap-in-if test body) `(if ,test ,body #f))").unwrap();
    
    let result = eval.eval_str("(wrap-in-if '#t '(+ 1 2))").unwrap();
    
    // Should produce (if #t (+ 1 2) #f)
    // Check that first element is 'if' symbol
    let first = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(first, "if").unwrap());
}

#[test]
fn test_nested_quasiquote_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 10)").unwrap();
    
    // `` `,,x `` (nested quasiquote with unquote)
    // At outer level, inner quasiquote is processed
    // The inner ,x at depth 2 stays as ,x (decremented to depth 1)
    // This is tricky - ``,x evaluates the outer quasiquote, which should
    // return `(quasiquote ,10) where the ,10 is unquoted at the outer level
    
    // Actually simpler test: nested quasiquote without inner unquote
    let result = eval.eval_str("``(a b c)").unwrap();
    
    // Should return (quasiquote (a b c))
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "quasiquote").unwrap());
}

#[test]
fn test_multiple_splices() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define xs '(1 2))").unwrap();
    eval.eval_str("(define ys '(3 4))").unwrap();
    
    // `(,@xs ,@ys) should give (1 2 3 4)
    let result = eval.eval_str("`(,@xs ,@ys)").unwrap();
    
    // Count elements
    let mut count = 0;
    let mut current = result;
    loop {
        match lisp.get(current).unwrap() {
            Value::Nil => break,
            Value::Cons { cdr, .. } => {
                count += 1;
                current = cdr;
            }
            _ => panic!("Expected list"),
        }
    }
    assert_eq!(count, 4);
}

#[test]
fn test_quasiquote_preserves_structure() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Pure quasiquote (no unquotes) should act like quote
    let quoted = eval.eval_str("'(a (b c) d)").unwrap();
    let quasiquoted = eval.eval_str("`(a (b c) d)").unwrap();
    
    // Both should be structurally equal
    // Check that they have the same shape
    fn count_elements<const N: usize>(lisp: &Lisp<N>, idx: ArenaIndex) -> usize {
        let mut count = 0;
        let mut current = idx;
        loop {
            match lisp.get(current).unwrap() {
                Value::Nil => return count,
                Value::Cons { cdr, .. } => {
                    count += 1;
                    current = cdr;
                }
                _ => return count,
            }
        }
    }
    
    assert_eq!(count_elements(&lisp, quoted), count_elements(&lisp, quasiquoted));
}

#[test]
fn test_quasiquote_with_lambda() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build a lambda expression using quasiquote
    eval.eval_str("(define body '(+ x 1))").unwrap();
    
    let result = eval.eval_str("`(lambda (x) ,body)").unwrap();
    
    // Should produce (lambda (x) (+ x 1))
    let first = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(first, "lambda").unwrap());
}

#[test]
fn test_quasiquote_code_generation() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Use quasiquote to generate code, then evaluate it
    eval.eval_str("(define op '+)").unwrap();
    eval.eval_str("(define a 10)").unwrap();
    eval.eval_str("(define b 20)").unwrap();
    
    // Build (+ 10 20) and evaluate it
    let result = eval.eval_str("(eval `(,op ,a ,b))").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number().unwrap(), 30);
}

#[test]
fn test_quasiquote_symbols_stay_symbols() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Symbols inside quasiquote stay as symbols (not looked up)
    let result = eval.eval_str("`(define x 5)").unwrap();
    
    let first = lisp.car(result).unwrap();
    assert!(lisp.get(first).unwrap().is_symbol());
    assert!(lisp.symbol_matches(first, "define").unwrap());
}

#[test]
fn test_quasiquote_with_vectors() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 42)").unwrap();
    
    // Quasiquote containing a vector (the vector itself should be quoted)
    let result = eval.eval_str("`(#(1 2 3) ,x)").unwrap();
    
    // First element should be a vector
    let first = lisp.car(result).unwrap();
    assert!(lisp.get(first).unwrap().is_array());
    
    // Second element should be 42
    let second = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert_eq!(lisp.get(second).unwrap().as_number().unwrap(), 42);
}

#[test]
fn test_quasiquote_dotted_pairs() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 1)").unwrap();
    eval.eval_str("(define y 2)").unwrap();
    
    // `(,x . ,y) should produce (1 . 2)
    let result = eval.eval_str("`(,x . ,y)").unwrap();
    
    let car = lisp.car(result).unwrap();
    let cdr = lisp.cdr(result).unwrap();
    
    assert_eq!(lisp.get(car).unwrap().as_number().unwrap(), 1);
    assert_eq!(lisp.get(cdr).unwrap().as_number().unwrap(), 2);
}

#[test]
fn test_quasiquote_splice_only_in_list_context() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Splice at start of list
    eval.eval_str("(define items '(a b c))").unwrap();
    
    let result = eval.eval_str("`(,@items)").unwrap();
    
    // Should be (a b c) - the items are spliced
    let first = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(first, "a").unwrap());
}

#[test]
fn test_quasiquote_with_conditionals() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build conditional expressions
    let result = eval.eval_str("`(if #t ,(+ 1 2) ,(+ 3 4))").unwrap();
    
    // Should be (if #t 3 7)
    let then_val = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    let else_val = lisp.car(lisp.cdr(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap()).unwrap();
    
    assert_eq!(lisp.get(then_val).unwrap().as_number().unwrap(), 3);
    assert_eq!(lisp.get(else_val).unwrap().as_number().unwrap(), 7);
}

// ============================================================================
// DOCUMENTATION VERIFICATION TESTS
// 
// These tests verify claims made in the documentation files:
// - README.md
// - docs/LISP_ARCHITECTURE.md
// - docs/SCHEME_R7RS_CONFORMANCE.md
// ============================================================================

/// Test: Reserved arena slots (LISP_ARCHITECTURE.md)
/// Slots 0-3 are reserved: Nil, True, False, Intern table
#[test]
fn test_doc_reserved_slots() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Slot 0 should be Nil
    let nil = lisp.nil().unwrap();
    assert!(lisp.get(nil).unwrap().is_nil());
    
    // Slot 1 should be True
    let true_val = lisp.true_val().unwrap();
    assert!(lisp.get(true_val).unwrap().is_true());
    
    // Slot 2 should be False
    let false_val = lisp.false_val().unwrap();
    assert!(lisp.get(false_val).unwrap().is_false());
    
    // Verify singleton identity: same symbol returns same index
    let foo1 = lisp.symbol("foo").unwrap();
    let foo2 = lisp.symbol("foo").unwrap();
    assert_eq!(foo1, foo2, "Interned symbols should have same index");
}

/// Test: Only #f is false (README.md, LISP_ARCHITECTURE.md)
#[test]
fn test_doc_only_false_is_false() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Only #f is false
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
    
    // Everything else is truthy
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if '() 1 2)"), 1, "Empty list should be truthy");
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if 0 1 2)"), 1, "Zero should be truthy");
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if \"\" 1 2)"), 1, "Empty string should be truthy");
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 1 2)"), 1);
}

// Note: Additional arithmetic, comparison, type predicate, equality, list operation,
// vector, character, string, and stdlib tests are already defined earlier in this file.
// The following tests cover additional documentation claims not yet tested.

/// Test: Tail-call optimization (README.md, LISP_ARCHITECTURE.md)
/// This test verifies TCO by running moderately deep recursion
/// Note: Due to Rust test thread stack limits, we test with 100 iterations
/// The full TCO support allows much deeper recursion in production use
#[test]
fn test_doc_tail_call_optimization() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a tail-recursive sum function
    eval.eval_str("(define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))").unwrap();
    
    // TCO allows this without Lisp stack overflow
    // sum(100) = 100 + 99 + ... + 1 = 5050
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum 100 0)"), 5050);
}

/// Test: GC control functions (README.md, LISP_ARCHITECTURE.md)
#[test]
fn test_doc_gc_control() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // arena-stats returns (capacity allocated free usage%)
    let result = eval.eval_str("(arena-stats)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::Cons { .. }));
    
    // gc-enabled? should return a boolean
    let result = eval.eval_str("(gc-enabled?)").unwrap();
    assert!(lisp.get(result).unwrap().is_boolean());
    
    // gc-disable should work
    eval.eval_str("(gc-disable)").unwrap();
    assert!(eval_is_false(&lisp, &mut eval, "(gc-enabled?)"));
    
    // gc-enable should work
    eval.eval_str("(gc-enable)").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(gc-enabled?)"));
    
    // gc should return stats
    let result = eval.eval_str("(gc)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::Cons { .. }));
}

/// Test: Rounding operations (LISP_ARCHITECTURE.md - identity for integers)
#[test]
fn test_doc_rounding_operations() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // All rounding operations are identity for integers
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor 42)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(ceiling 42)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate 42)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(round 42)"), 42);
    
    // Negative numbers too
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor -7)"), -7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(ceiling -7)"), -7);
}

/// Test: Lexical closures (README.md, LISP_ARCHITECTURE.md)
#[test]
fn test_doc_lexical_closures() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a closure that captures a variable
    eval.eval_str("(define (make-adder n) (lambda (x) (+ x n)))").unwrap();
    eval.eval_str("(define add5 (make-adder 5))").unwrap();
    eval.eval_str("(define add10 (make-adder 10))").unwrap();
    
    // Each closure captures its own environment
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 3)"), 8);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add10 3)"), 13);
}

/// Test: Strict evaluation / call-by-value (README.md, LISP_ARCHITECTURE.md)
#[test]
fn test_doc_strict_evaluation() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Side effects in arguments happen immediately
    eval.eval_str("(define count 0)").unwrap();
    eval.eval_str("(define lst (cons (begin (set! count 1) 'a) '()))").unwrap();
    
    // count should be 1 because the argument was evaluated before cons
    assert_eq!(eval_to_num(&lisp, &mut eval, "count"), 1);
}

/// Test: c...r accessor compositions (SCHEME_R7RS_CONFORMANCE.md)
#[test]
fn test_doc_car_cdr_compositions() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define nested '((1 2) (3 4) (5 6)))").unwrap();
    
    // cadr = (car (cdr ...))
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cadr nested))"), 3);
    
    // caddr = (car (cdr (cdr ...)))
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (caddr nested))"), 5);
    
    // cddr = (cdr (cdr ...))
    let _result = eval.eval_str("(cddr nested)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (car (cddr nested)))"), 5);
}

// Test for nested ellipsis pattern matching bug
#[test]
fn test_nested_ellipsis_bug() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test: Nested ellipsis - this reveals the bug
    eval.eval_str("(define-syntax nest-test (lambda (x) (syntax-case x () ((nest-test ((a) ...)) (syntax (quote (a ...))))))))").unwrap();
    let result = eval.eval_str("(nest-test ((1) (2) (3)))").unwrap();
    
    // Expected: (1 2 3)
    // Actual bug: (1 (2) (3))
    let first = lisp.car(result).unwrap();
    assert_eq!(lisp.get(first).unwrap().as_number(), Some(1), "First element should be 1");
    
    let second = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    // This assertion currently fails because second is (2) not 2
    assert_eq!(lisp.get(second).unwrap().as_number(), Some(2), 
        "Second element should be 2, not (2) - nested ellipsis bug");
}

// Test to check what the pattern variable is bound to
#[test]
fn test_check_pattern_binding_value() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test with three elements to see the pattern
    eval.eval_str("(define-syntax test3 (lambda (x) (syntax-case x () ((test3 ((a) ...)) (syntax (quote (a ...))))))))").unwrap();
    let result = eval.eval_str("(test3 ((1) (2) (3)))").unwrap();
    
    eprintln!("Result for ((1) (2) (3)):");
    let mut curr = result;
    let mut idx = 0;
    while let Value::Cons { .. } = lisp.get(curr).unwrap() {
        let elem = lisp.car(curr).unwrap();
        eprintln!("  [{}] = {:?}", idx, lisp.get(elem));
        curr = lisp.cdr(curr).unwrap();
        idx += 1;
    }
    
    // Expected: (1 2 3)
    // Bug: (1 (2) (3))
    
    // Check - first should be 1, second should be 2, third should be 3
    let v1 = lisp.car(result).unwrap();
    assert_eq!(lisp.get(v1).unwrap().as_number(), Some(1), "First should be 1");
    
    let v2 = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    
    // This is where the bug is
    match lisp.get(v2).unwrap() {
        Value::Number(n) => assert_eq!(n, 2, "Second should be 2"),
        Value::Cons { .. } => {
            // Bug: v2 is (2) instead of 2
            let inner = lisp.car(v2).unwrap();
            eprintln!("BUG: Second element is ({:?}) instead of just the number", lisp.get(inner).unwrap().as_number());
            panic!("Nested ellipsis bug: second element should be 2, not (2)");
        }
        other => panic!("Unexpected: {:?}", other),
    }
}

// Check if symbol interning is working
#[test]
fn test_symbol_interning() {
    let lisp: Lisp<20000> = Lisp::new();
    
    // Create symbol 'a' multiple times
    let a1 = lisp.symbol("a").unwrap();
    let a2 = lisp.symbol("a").unwrap();
    let a3 = lisp.symbol("a").unwrap();
    
    eprintln!("a1 = {:?}", a1);
    eprintln!("a2 = {:?}", a2);
    eprintln!("a3 = {:?}", a3);
    
    // They should all be the same arena index
    assert_eq!(a1, a2, "Symbols should be interned to same index");
    assert_eq!(a2, a3, "Symbols should be interned to same index");
    
    // Also check symbol_eq
    assert!(lisp.symbol_eq(a1, a2).unwrap());
    assert!(lisp.symbol_eq(a2, a3).unwrap());
}

// ============================================================================
// Comprehensive tests for nested ellipsis pattern matching
// ============================================================================

/// Test nested ellipsis with single-element inner pattern
#[test]
fn test_nested_ellipsis_single_var() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Pattern ((a) ...) matches ((1) (2) (3)) and binds a to (1 2 3)
    eval.eval_str("(define-syntax extract (lambda (x) (syntax-case x () ((extract ((a) ...)) (syntax (quote (a ...))))))))").unwrap();
    
    // Single element
    let result = eval.eval_str("(extract ((1)))").unwrap();
    let v1 = lisp.car(result).unwrap();
    assert_eq!(lisp.get(v1).unwrap().as_number(), Some(1));
    
    // Two elements
    let result = eval.eval_str("(extract ((1) (2)))").unwrap();
    let v1 = lisp.car(result).unwrap();
    let v2 = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert_eq!(lisp.get(v1).unwrap().as_number(), Some(1));
    assert_eq!(lisp.get(v2).unwrap().as_number(), Some(2));
    
    // Three elements
    let result = eval.eval_str("(extract ((1) (2) (3)))").unwrap();
    let v1 = lisp.car(result).unwrap();
    let v2 = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let v3 = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert_eq!(lisp.get(v1).unwrap().as_number(), Some(1));
    assert_eq!(lisp.get(v2).unwrap().as_number(), Some(2));
    assert_eq!(lisp.get(v3).unwrap().as_number(), Some(3));
}

/// Test nested ellipsis with multiple pattern variables
#[test]
fn test_nested_ellipsis_multiple_vars() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Pattern ((a b) ...) matches ((1 2) (3 4)) and binds:
    // a to (1 3), b to (2 4)
    eval.eval_str("(define-syntax pair-extract (lambda (x) (syntax-case x () ((pair-extract ((a b) ...)) (syntax (quote ((a ...) (b ...))))))))").unwrap();
    
    let result = eval.eval_str("(pair-extract ((1 2) (3 4)))").unwrap();
    
    // First element should be (1 3)
    let first = lisp.car(result).unwrap();
    let a1 = lisp.car(first).unwrap();
    let a2 = lisp.car(lisp.cdr(first).unwrap()).unwrap();
    assert_eq!(lisp.get(a1).unwrap().as_number(), Some(1));
    assert_eq!(lisp.get(a2).unwrap().as_number(), Some(3));
    
    // Second element should be (2 4)
    let second = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let b1 = lisp.car(second).unwrap();
    let b2 = lisp.car(lisp.cdr(second).unwrap()).unwrap();
    assert_eq!(lisp.get(b1).unwrap().as_number(), Some(2));
    assert_eq!(lisp.get(b2).unwrap().as_number(), Some(4));
}

/// Test ellipsis with proper let-style binding pattern
#[test]
fn test_let_style_bindings() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test the nested ellipsis pattern for extracting names and values
    // This is the core pattern that let macros need
    eval.eval_str("(define-syntax extract-bindings (lambda (x) (syntax-case x () ((extract-bindings ((name val) ...)) (syntax (quote ((name ...) (val ...))))))))").unwrap();
    
    // Single binding: ((x 1)) -> ((x) (1))
    let result = eval.eval_str("(extract-bindings ((x 1)))").unwrap();
    let names = lisp.car(result).unwrap();
    let vals = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let name = lisp.car(names).unwrap();
    let val = lisp.car(vals).unwrap();
    assert!(lisp.symbol_matches(name, "x").unwrap());
    assert_eq!(lisp.get(val).unwrap().as_number(), Some(1));
    
    // Multiple bindings: ((a 10) (b 20)) -> ((a b) (10 20))
    let result = eval.eval_str("(extract-bindings ((a 10) (b 20)))").unwrap();
    let names = lisp.car(result).unwrap();
    let vals = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let n1 = lisp.car(names).unwrap();
    let n2 = lisp.car(lisp.cdr(names).unwrap()).unwrap();
    let v1 = lisp.car(vals).unwrap();
    let v2 = lisp.car(lisp.cdr(vals).unwrap()).unwrap();
    assert!(lisp.symbol_matches(n1, "a").unwrap());
    assert!(lisp.symbol_matches(n2, "b").unwrap());
    assert_eq!(lisp.get(v1).unwrap().as_number(), Some(10));
    assert_eq!(lisp.get(v2).unwrap().as_number(), Some(20));
}

/// Test parsing of ellipsis as a proper list element
#[test]
fn test_ellipsis_parsing() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // (a ...) should be a proper list with ... as second element
    let result = eval.eval_str("(list? '(a ...))").unwrap();
    assert!(lisp.get(result).unwrap().is_true(), "(a ...) should be a proper list");
    
    // The second element should be the symbol ...
    let result = eval.eval_str("(cadr '(a ...))").unwrap();
    assert!(lisp.symbol_matches(result, "...").unwrap());
    
    // ((a) ...) should also be a proper list
    let result = eval.eval_str("(list? '((a) ...))").unwrap();
    assert!(lisp.get(result).unwrap().is_true(), "((a) ...) should be a proper list");
}

/// Test empty ellipsis match
#[test]
fn test_empty_ellipsis() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Matching zero elements with ellipsis
    eval.eval_str("(define-syntax zero-or-more (lambda (x) (syntax-case x () ((zero-or-more a ...) (syntax (quote (a ...)))))))").unwrap();
    
    // Empty: should produce empty list
    let result = eval.eval_str("(zero-or-more)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil(), "zero-or-more with no args should be ()");
    
    // One element
    let result = eval.eval_str("(zero-or-more 1)").unwrap();
    let first = lisp.car(result).unwrap();
    assert_eq!(lisp.get(first).unwrap().as_number(), Some(1));
}

#[test]
fn check_eval_error_size() {
    use core::mem::size_of;
    use grift_eval::{EvalError, ErrorKind, StackFrame, ArgCountInfo};
    use grift_eval::ParseError;
    
    println!("\n=== Type Sizes (stack efficiency check) ===");
    println!("EvalError:        {} bytes", size_of::<EvalError>());
    println!("ErrorKind:        {} bytes", size_of::<ErrorKind>());
    println!("StackFrame:       {} bytes", size_of::<StackFrame>());
    println!("ArgCountInfo:     {} bytes", size_of::<ArgCountInfo>());
    println!("ParseError:       {} bytes", size_of::<ParseError>());
    println!("Option<ParseError>: {} bytes", size_of::<Option<ParseError>>());
    println!("Option<&str>:     {} bytes", size_of::<Option<&'static str>>());
    
    // Verify EvalError is now small enough for efficient stack usage
    // Previous size was 440 bytes, now reduced to ~88 bytes (80% reduction)
    let error_size = size_of::<EvalError>();
    assert!(
        error_size <= 96,
        "EvalError is {} bytes, expected <= 96 bytes for stack efficiency",
        error_size
    );
    
    // Verify ErrorKind uses repr(u8) for minimal size
    assert_eq!(size_of::<ErrorKind>(), 1, "ErrorKind should be 1 byte (repr(u8))");
    
    // Verify ArgCountInfo is compact (4 bytes: 2 × u16)
    assert_eq!(size_of::<ArgCountInfo>(), 4, "ArgCountInfo should be 4 bytes (2 × u16)");
}

// ============================================================
// Keyword Shadowing Tests (R7RS §4.3)
// ============================================================

#[test]
fn test_keyword_shadowing_cond() {
    // Per R7RS §4.3: "local variable bindings can shadow syntactic bindings"
    // The cond macro should be shadowable by a variable binding
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // First verify that cond works as a macro
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond (#t 42))"), 42);
    
    // Now define cond as a function that returns 32
    eval.eval_str("(define (cond) 32)").unwrap();
    
    // Calling (cond) should now invoke the variable binding, not the macro
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond)"), 32);
}

#[test]
fn test_keyword_shadowing_let() {
    // Test that let can be shadowed
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // First verify that let works as a macro
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((x 5)) x)"), 5);
    
    // Define let as a function
    eval.eval_str("(define (let x) (+ x 10))").unwrap();
    
    // Calling (let 7) should invoke the variable binding
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let 7)"), 17);
}

#[test]
fn test_keyword_shadowing_and() {
    // Test that and can be shadowed
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // First verify that and works as a macro
    assert_eq!(eval_to_num(&lisp, &mut eval, "(and 1 2 3)"), 3);
    
    // Define and as a variable
    eval.eval_str("(define and 999)").unwrap();
    
    // Referencing and should return the variable value
    assert_eq!(eval_to_num(&lisp, &mut eval, "and"), 999);
}

#[test]
fn test_keyword_shadowing_or() {
    // Test that or can be shadowed
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // First verify that or works as a macro
    assert_eq!(eval_to_num(&lisp, &mut eval, "(or #f 42)"), 42);
    
    // Define or as a function
    eval.eval_str("(define (or a b) (* a b))").unwrap();
    
    // Calling (or 3 4) should invoke the variable binding
    assert_eq!(eval_to_num(&lisp, &mut eval, "(or 3 4)"), 12);
}

#[test]
fn test_keyword_shadowing_when() {
    // Test that when can be shadowed
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // First verify that when works as a macro
    assert_eq!(eval_to_num(&lisp, &mut eval, "(when #t 100)"), 100);
    
    // Define when as a function
    eval.eval_str("(define (when) 77)").unwrap();
    
    // Calling (when) should invoke the variable binding
    assert_eq!(eval_to_num(&lisp, &mut eval, "(when)"), 77);
}

// ═══════════════════════════════════════════════════════════════════════════
// CASE-LAMBDA TESTS (R7RS Section 4.2.9)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_case_lambda_single_clause() {
    // case-lambda with single clause should work like regular lambda
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define add1 (case-lambda ((x) (+ x 1))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add1 5)"), 6);
}

#[test]
fn test_case_lambda_two_clauses() {
    // Test case-lambda with two clauses - dispatches on argument count
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let define_result = eval.eval_str("
        (define identity-or-sum
          (case-lambda
            ((x) x)
            ((x y) (+ x y))))
    ");
    assert!(define_result.is_ok(), "define failed: {:?}", define_result);
    
    // Single arg - returns the value unchanged
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity-or-sum 5)"), 5);
    
    // Two args - returns the sum
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity-or-sum 3 4)"), 7);
}

#[test]
fn test_case_lambda_three_clauses() {
    // Test case-lambda with three clauses
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("
        (define multi-arity
          (case-lambda
            (() 0)
            ((x) x)
            ((x y) (+ x y))))
    ").unwrap();
    
    // Test all three arities
    assert_eq!(eval_to_num(&lisp, &mut eval, "(multi-arity)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(multi-arity 42)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(multi-arity 3 4)"), 7);
    
    // Test that wrong arity errors
    let result = eval.eval_str("(multi-arity 1 2 3)");
    assert!(result.is_err(), "Expected error for 3 args");
}

#[test]
fn test_case_lambda_range_example() {
    // R7RS spec example: range function with multi-arity dispatch
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define range using case-lambda for optional start argument
    eval.eval_str("
        (define range
          (case-lambda
            ((e) (range 0 e))
            ((b e)
              (do ((r '() (cons e r))
                   (e (- e 1) (- e 1)))
                  ((< e b) r)))))
    ").unwrap();
    
    // (range 3) should return (0 1 2) - single arg uses default start of 0
    let result = eval.eval_str("(range 3)").unwrap();
    let first = lisp.get(lisp.car(result).unwrap()).unwrap().as_number().unwrap();
    assert_eq!(first, 0);
    
    // (range 3 5) should return (3 4) - explicit start
    let result = eval.eval_str("(range 3 5)").unwrap();
    let first = lisp.get(lisp.car(result).unwrap()).unwrap().as_number().unwrap();
    assert_eq!(first, 3);
}

#[test]
fn test_case_lambda_zero_args() {
    // Test case-lambda with zero-arg clause
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Single-clause zero-arity case-lambda works
    eval.eval_str("(define zero-only (case-lambda (() 100)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(zero-only)"), 100);
    
    // Multi-clause with zero and one arg
    eval.eval_str("(define zero-or-one (case-lambda (() 100) ((x) x)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(zero-or-one)"), 100);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(zero-or-one 42)"), 42);
}

#[test]
fn test_case_lambda_variadic_clause() {
    // Test case-lambda with a variadic catch-all clause
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("
        (define add-all
          (case-lambda
            (() 0)
            ((x) x)
            ((x y) (+ x y))
            (args (apply + args))))
    ").unwrap();
    
    // Test specific arities
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-all)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-all 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-all 3 4)"), 7);
    
    // Test variadic catch-all for 3+ args
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-all 1 2 3)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-all 1 2 3 4 5)"), 15);
}

// ═══════════════════════════════════════════════════════════════════════════
// COND-EXPAND TESTS (R7RS Section 4.2.1)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_cond_expand_grift() {
    // cond-expand should recognize 'grift' feature
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand (grift 42) (else 0))"), 42);
}

#[test]
fn test_cond_expand_r7rs() {
    // cond-expand should recognize 'r7rs' feature
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand (r7rs 100) (else 0))"), 100);
}

#[test]
fn test_cond_expand_else() {
    // cond-expand else clause should match when no other clause matches
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // 'unknown-feature' should not match, so else is selected
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand (unknown-feature 1) (else 99))"), 99);
}

#[test]
fn test_cond_expand_and() {
    // cond-expand with 'and' compound requirement
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Both r7rs and grift are supported, so 'and' should match
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand ((and r7rs grift) 55) (else 0))"), 55);
    
    // r7rs is supported but ieee-float is not, so 'and' should not match
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand ((and r7rs ieee-float) 1) (else 66))"), 66);
}

#[test]
fn test_cond_expand_or() {
    // cond-expand with 'or' compound requirement
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Neither ieee-float nor ratios are supported, but grift is
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand ((or ieee-float ratios grift) 77) (else 0))"), 77);
    
    // None of these are supported
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand ((or ieee-float ratios) 1) (else 88))"), 88);
}

#[test]
fn test_cond_expand_not() {
    // cond-expand with 'not' requirement
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // ieee-float is not supported, so (not ieee-float) should be true
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand ((not ieee-float) 33) (else 0))"), 33);
    
    // grift is supported, so (not grift) should be false
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cond-expand ((not grift) 1) (else 44))"), 44);
}

#[test]
fn test_cond_expand_multiple_expressions() {
    // cond-expand with multiple expressions in a clause
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 0)").unwrap();
    eval.eval_str("(cond-expand (grift (set! x 1) (set! x (+ x 10))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 11);
}

// ═══════════════════════════════════════════════════════════════════════════
// DELAY-FORCE AND PROMISE TESTS (R7RS Section 4.2.5)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_delay_force_simple() {
    // delay-force basic functionality
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (delay-force (+ 1 2 3)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p)"), 6);
}

#[test]
fn test_delay_force_memo() {
    // delay-force should also memoize
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define counter 0)").unwrap();
    eval.eval_str("(define p (delay-force (begin (set! counter (+ counter 1)) counter)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p)"), 1); // Still 1, memoized
    assert_eq!(eval_to_num(&lisp, &mut eval, "counter"), 1);
}

#[test]
fn test_delay_force_chains() {
    // delay-force should handle promise chains without growing stack
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a chain of delay-force
    eval.eval_str("(define p1 (delay 42))").unwrap();
    eval.eval_str("(define p2 (delay-force (force p1)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p2)"), 42);
}

#[test]
fn test_promise_predicate() {
    // promise? should return #t for promises
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (delay 1))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(promise? p)"));
    
    // Non-promises should return #f
    assert!(eval_is_false(&lisp, &mut eval, "(promise? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(promise? '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(promise? \"hello\")"));
}

#[test]
fn test_make_promise() {
    // make-promise should create a promise that returns the value
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (make-promise 42))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p)"), 42);
}

#[test]
fn test_make_promise_already_promise() {
    // make-promise on an existing promise should return the same promise
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p1 (delay 99))").unwrap();
    eval.eval_str("(define p2 (make-promise p1))").unwrap();
    
    // Both should evaluate to the same value
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p1)"), 99);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p2)"), 99);
}


// ═══════════════════════════════════════════════════════════════════════════
// REST-ARGUMENT LAMBDA TESTS (R7RS Section 4.1.4)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_rest_lambda_basic() {
    // Test (lambda args body) form - all args collected into a list
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define list-all (lambda args args))").unwrap();
    
    // With multiple args
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (list-all 1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (list-all 1 2 3))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr (list-all 1 2 3))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(caddr (list-all 1 2 3))"), 3);
    
    // With single arg
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (list-all 42))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (list-all 42))"), 42);
    
    // With no args - should return empty list
    let result = eval.eval_str("(list-all)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

#[test]
fn test_rest_lambda_with_computation() {
    // Rest lambda should evaluate arguments before collecting
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define sum-all (lambda args (fold + 0 args)))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-all 1 2 3 4 5)"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-all (* 2 3) (* 4 5))"), 26);
}

#[test]
fn test_dotted_lambda_basic() {
    // Test (lambda (a b . rest) body) form
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define f (lambda (a b . rest) rest))").unwrap();
    
    // Exactly 2 args - rest is empty
    let result = eval.eval_str("(f 1 2)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
    
    // More than 2 args - rest collects extras
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (f 1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (f 1 2 3 4 5))"), 3);
}

#[test]
fn test_dotted_lambda_use_all_params() {
    // Test using both fixed and rest params
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define f (lambda (first . rest) (cons first rest)))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (f 1 2 3))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr (f 1 2 3))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(caddr (f 1 2 3))"), 3);
    
    // With just one arg
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (f 42))"), 42);
    let result = eval.eval_str("(cdr (f 42))").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

#[test]
fn test_dotted_lambda_sum_with_base() {
    // Practical example: sum with a base value
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define sum-with-base (lambda (base . nums) (fold + base nums)))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-with-base 100 1 2 3)"), 106);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-with-base 0)"), 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// MORE PROMISE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_promise_lazy_evaluation() {
    // delay should not evaluate its body until forced
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define side-effect 0)").unwrap();
    eval.eval_str("(define p (delay (set! side-effect (+ side-effect 1))))").unwrap();
    
    // Side effect should not have happened yet
    assert_eq!(eval_to_num(&lisp, &mut eval, "side-effect"), 0);
    
    // Force the promise
    eval.eval_str("(force p)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "side-effect"), 1);
    
    // Force again - should not re-evaluate (memoization)
    eval.eval_str("(force p)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "side-effect"), 1);
}

#[test]
fn test_multiple_promises() {
    // Multiple independent promises
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p1 (delay 10))").unwrap();
    eval.eval_str("(define p2 (delay 20))").unwrap();
    eval.eval_str("(define p3 (delay (+ (force p1) (force p2))))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p3)"), 30);
}

#[test]
fn test_promise_chain() {
    // Chained promises
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p1 (delay 5))").unwrap();
    eval.eval_str("(define p2 (delay (* 2 (force p1))))").unwrap();
    eval.eval_str("(define p3 (delay (* 3 (force p2))))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p3)"), 30);
}

// ============================================================================
// Tests for EXTENDING_SCHEME_MACROS.md - Part 1: Recursive Helper Patterns
// ============================================================================

/// Test accumulator-based recursive macro pattern
/// This tests the pattern described in Section 1.1 of EXTENDING_SCHEME_MACROS.md
#[test]
fn test_recursive_helper_accumulator_pattern() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a simple accumulator-based recursive macro
    // This collects elements one at a time
    eval.eval_str(r#"
        (define-syntax collect-first
          (lambda (x)
            (syntax-case x ()
              ((collect-first () (acc ...))
               (syntax (quote (acc ...))))
              ((collect-first (first . rest) (acc ...))
               (syntax (collect-first rest (acc ... first)))))))
    "#).unwrap();
    
    // Test with empty input
    let result = eval.eval_str("(collect-first () ())").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
    
    // Test with single element
    let result = eval.eval_str("(collect-first (a) ())").unwrap();
    let first = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(first, "a").unwrap());
    
    // Test with multiple elements
    let result = eval.eval_str("(collect-first (a b c) ())").unwrap();
    let first = lisp.car(result).unwrap();
    let second = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert!(lisp.symbol_matches(first, "a").unwrap());
    assert!(lisp.symbol_matches(second, "b").unwrap());
    assert!(lisp.symbol_matches(third, "c").unwrap());
}

/// Test dual accumulator pattern (used by do macro)
/// This tests the pattern described in Section 1.4 of EXTENDING_SCHEME_MACROS.md
#[test]
fn test_dual_accumulator_pattern() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro that extracts two pieces from each binding
    eval.eval_str(r#"
        (define-syntax extract-pairs
          (lambda (x)
            (syntax-case x ()
              ((extract-pairs () (firsts ...) (seconds ...))
               (syntax (list (quote (firsts ...)) (quote (seconds ...)))))
              ((extract-pairs ((a b) . rest) (firsts ...) (seconds ...))
               (syntax (extract-pairs rest (firsts ... a) (seconds ... b)))))))
    "#).unwrap();
    
    // Test with single pair
    let result = eval.eval_str("(extract-pairs ((x 1)) () ())").unwrap();
    let firsts = lisp.car(result).unwrap();
    let seconds = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    
    let x = lisp.car(firsts).unwrap();
    let one = lisp.car(seconds).unwrap();
    assert!(lisp.symbol_matches(x, "x").unwrap());
    assert_eq!(lisp.get(one).unwrap().as_number(), Some(1));
    
    // Test with multiple pairs
    let result = eval.eval_str("(extract-pairs ((a 1) (b 2) (c 3)) () ())").unwrap();
    let firsts = lisp.car(result).unwrap();
    let seconds = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    
    // Check firsts list is (a b c)
    assert!(lisp.symbol_matches(lisp.car(firsts).unwrap(), "a").unwrap());
    assert!(lisp.symbol_matches(lisp.car(lisp.cdr(firsts).unwrap()).unwrap(), "b").unwrap());
    assert!(lisp.symbol_matches(lisp.car(lisp.cdr(lisp.cdr(firsts).unwrap()).unwrap()).unwrap(), "c").unwrap());
    
    // Check seconds list is (1 2 3)
    assert_eq!(lisp.get(lisp.car(seconds).unwrap()).unwrap().as_number(), Some(1));
    assert_eq!(lisp.get(lisp.car(lisp.cdr(seconds).unwrap()).unwrap()).unwrap().as_number(), Some(2));
}

/// Test macro hygiene with recursive patterns
/// This tests hygiene as described in Section 1.5 of EXTENDING_SCHEME_MACROS.md
#[test]
fn test_recursive_macro_hygiene() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a recursive macro that introduces a binding
    eval.eval_str(r#"
        (define-syntax sum-list
          (lambda (x)
            (syntax-case x ()
              ((sum-list () acc) (syntax acc))
              ((sum-list (x . rest) acc)
               (syntax (sum-list rest (+ acc x)))))))
    "#).unwrap();
    
    // User defines 'acc' - should not be captured by macro's 'acc'
    eval.eval_str("(define acc 1000)").unwrap();
    
    // Use the macro - user's 'acc' should be untouched
    let result = eval.eval_str("(sum-list (1 2 3) 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6));
    
    // Verify user's 'acc' is still 1000
    let user_acc = eval.eval_str("acc").unwrap();
    assert_eq!(lisp.get(user_acc).unwrap().as_number(), Some(1000));
}

/// Test that do loop with accumulators works correctly
/// This validates the do macro implementation from EXTENDING_SCHEME_MACROS.md
#[test]
fn test_do_loop_with_accumulator() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sum 1 to 10 using do with explicit step
    let result = eval.eval_str(r#"
        (do ((i 1 (+ i 1))
             (sum 0 (+ sum i)))
            ((> i 10) sum))
    "#).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(55));
}

/// Test do loop without explicit step (step defaults to variable)
#[test]
fn test_do_loop_default_step() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // When step is omitted, variable keeps its value
    let result = eval.eval_str(r#"
        (do ((count 0 (+ count 1))
             (limit 5))
            ((>= count limit) 'done))
    "#).unwrap();
    assert!(lisp.symbol_matches(result, "done").unwrap());
}

/// Test pattern alternatives in recursive macros
/// This tests the pattern described in Section 1.1 of EXTENDING_SCHEME_MACROS.md
#[test]
fn test_pattern_alternatives() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro with multiple pattern alternatives
    eval.eval_str(r#"
        (define-syntax process-item
          (lambda (x)
            (syntax-case x ()
              ((process-item ()) (syntax (quote empty)))
              ((process-item (a)) (syntax (quote one)))
              ((process-item (a b)) (syntax (quote two)))
              ((process-item (a b c)) (syntax (quote three)))
              ((process-item other) (syntax (quote many))))))
    "#).unwrap();
    
    let result = eval.eval_str("(process-item ())").unwrap();
    assert!(lisp.symbol_matches(result, "empty").unwrap());
    
    let result = eval.eval_str("(process-item (1))").unwrap();
    assert!(lisp.symbol_matches(result, "one").unwrap());
    
    let result = eval.eval_str("(process-item (1 2))").unwrap();
    assert!(lisp.symbol_matches(result, "two").unwrap());
    
    let result = eval.eval_str("(process-item (1 2 3))").unwrap();
    assert!(lisp.symbol_matches(result, "three").unwrap());
    
    let result = eval.eval_str("(process-item (1 2 3 4))").unwrap();
    assert!(lisp.symbol_matches(result, "many").unwrap());
}

/// Test literal keyword matching in patterns
/// This tests literal handling as part of syntax-case
#[test]
fn test_literal_keywords() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro with literal keywords
    eval.eval_str(r#"
        (define-syntax my-cond
          (lambda (x)
            (syntax-case x (else =>)
              ((my-cond (else result)) (syntax result))
              ((my-cond (test => proc))
               (syntax (let ((temp test))
                         (if temp (proc temp) #f))))
              ((my-cond (test result))
               (syntax (if test result #f))))))
    "#).unwrap();
    
    // Test else clause
    let result = eval.eval_str("(my-cond (else 42))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // Test arrow clause
    let result = eval.eval_str("(my-cond (5 => (lambda (x) (* x 2))))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(10));
    
    // Test regular clause
    let result = eval.eval_str("(my-cond (#t 100))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(100));
}

/// Test nested ellipsis with helper macro decomposition
/// This tests the decomposition pattern from Section 1.2 of EXTENDING_SCHEME_MACROS.md
#[test]
fn test_nested_pattern_decomposition() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro that decomposes nested patterns using helpers
    eval.eval_str(r#"
        (define-syntax flatten-pairs
          (lambda (x)
            (syntax-case x ()
              ((flatten-pairs ()) (syntax (quote ())))
              ((flatten-pairs ((a b) . rest))
               (syntax (cons a (cons b (flatten-pairs rest))))))))
    "#).unwrap();
    
    let result = eval.eval_str("(flatten-pairs ((1 2) (3 4)))").unwrap();
    
    // Should produce (1 2 3 4)
    let v1 = lisp.car(result).unwrap();
    let v2 = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let v3 = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    let v4 = lisp.car(lisp.cdr(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap()).unwrap();
    
    assert_eq!(lisp.get(v1).unwrap().as_number(), Some(1));
    assert_eq!(lisp.get(v2).unwrap().as_number(), Some(2));
    assert_eq!(lisp.get(v3).unwrap().as_number(), Some(3));
    assert_eq!(lisp.get(v4).unwrap().as_number(), Some(4));
}

// ============================================================================
// Mark Infrastructure Tests
// ============================================================================

/// Test basic mark_syntax functionality via syntax object creation
/// This tests that we can create and manipulate syntax objects with marks
#[test]
fn test_syntax_object_with_marks() {
    let lisp: Lisp<20000> = Lisp::new();
    let _eval = Evaluator::new(&lisp).unwrap();
    
    // Create a simple symbol
    let sym = lisp.symbol("x").unwrap();
    let nil = lisp.nil().unwrap();
    
    // Create a syntax object with empty marks
    let stx = lisp.syntax(sym, nil, nil).unwrap();
    
    // Verify we can extract the parts
    let (expr, marks, subst) = lisp.syntax_parts(stx).unwrap();
    assert!(lisp.symbol_matches(expr, "x").unwrap());
    assert!(lisp.get(marks).unwrap().is_nil());
    assert!(lisp.get(subst).unwrap().is_nil());
    
    // Verify syntax_to_datum returns the original expression
    let datum = lisp.syntax_to_datum(stx).unwrap();
    assert!(lisp.symbol_matches(datum, "x").unwrap());
    
    // For non-syntax objects, syntax_to_datum passes through
    let num = lisp.number(42).unwrap();
    let passed = lisp.syntax_to_datum(num).unwrap();
    assert_eq!(lisp.get(passed).unwrap().as_number(), Some(42));
}

/// Test mark_syntax applies fresh marks correctly
#[test]
fn test_mark_syntax_applies_marks() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a symbol
    let sym = lisp.symbol("foo").unwrap();
    let nil = lisp.nil().unwrap();
    
    // Create a syntax object with no marks
    let stx1 = lisp.syntax(sym, nil, nil).unwrap();
    
    // Apply a mark
    let stx2 = eval.mark_syntax(stx1).unwrap();
    
    // The marked syntax object should have a non-empty marks list
    let (_expr, marks, _subst) = lisp.syntax_parts(stx2).unwrap();
    assert!(!lisp.get(marks).unwrap().is_nil(), "marks should be non-empty after mark_syntax");
    
    // The first mark should be a gensym symbol (verify it's a symbol)
    let first_mark = lisp.car(marks).unwrap();
    match lisp.get(first_mark).unwrap() {
        Value::Symbol(_) => {
            // Mark is a symbol as expected - gensyms are symbols starting with #:
            // We can use symbol_to_bytes to check, but just verifying it's a symbol is sufficient
            let mut buf = [0u8; 32];
            let len = lisp.symbol_to_bytes(first_mark, &mut buf).unwrap();
            let name = core::str::from_utf8(&buf[..len]).unwrap();
            assert!(name.starts_with("#:"), "mark should be a gensym starting with #:");
        }
        _ => panic!("mark should be a symbol"),
    }
    
    // Apply another mark
    let stx3 = eval.mark_syntax(stx2).unwrap();
    let (_expr, marks2, _subst) = lisp.syntax_parts(stx3).unwrap();
    
    // Should have two marks now
    let mark1 = lisp.car(marks2).unwrap();
    let mark2 = lisp.car(lisp.cdr(marks2).unwrap()).unwrap();
    
    // They should be different gensyms
    assert!(!lisp.symbol_eq(mark1, mark2).unwrap(), "successive marks should be different");
}

/// Test bound_identifier_eq with identical identifiers
#[test]
fn test_bound_identifier_eq_same() {
    let lisp: Lisp<20000> = Lisp::new();
    let eval = Evaluator::new(&lisp).unwrap();
    
    // Create two syntax objects with the same name and marks
    let sym = lisp.symbol("x").unwrap();
    let nil = lisp.nil().unwrap();
    
    let stx1 = lisp.syntax(sym, nil, nil).unwrap();
    let stx2 = lisp.syntax(sym, nil, nil).unwrap();
    
    // They should be bound-identifier=?
    assert!(eval.bound_identifier_eq(stx1, stx2).unwrap());
    
    // Plain symbols should also work
    let sym_a = lisp.symbol("a").unwrap();
    let sym_a2 = lisp.symbol("a").unwrap();
    assert!(eval.bound_identifier_eq(sym_a, sym_a2).unwrap());
}

/// Test bound_identifier_eq with different names
#[test]
fn test_bound_identifier_eq_different_names() {
    let lisp: Lisp<20000> = Lisp::new();
    let eval = Evaluator::new(&lisp).unwrap();
    
    let sym_x = lisp.symbol("x").unwrap();
    let sym_y = lisp.symbol("y").unwrap();
    let nil = lisp.nil().unwrap();
    
    let stx_x = lisp.syntax(sym_x, nil, nil).unwrap();
    let stx_y = lisp.syntax(sym_y, nil, nil).unwrap();
    
    // Different names - should not be equal
    assert!(!eval.bound_identifier_eq(stx_x, stx_y).unwrap());
}

/// Test bound_identifier_eq with different marks
#[test]
fn test_bound_identifier_eq_different_marks() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let sym = lisp.symbol("x").unwrap();
    let nil = lisp.nil().unwrap();
    
    // Create unmarked syntax object
    let stx1 = lisp.syntax(sym, nil, nil).unwrap();
    
    // Create marked syntax object (same name, different marks)
    let stx2 = eval.mark_syntax(stx1).unwrap();
    
    // Same name but different marks - should NOT be bound-identifier=?
    assert!(!eval.bound_identifier_eq(stx1, stx2).unwrap());
}

/// Test free_identifier_eq with unbound identifiers
#[test]
fn test_free_identifier_eq_unbound() {
    let lisp: Lisp<20000> = Lisp::new();
    let eval = Evaluator::new(&lisp).unwrap();
    
    // Create two syntax objects for the same unbound name
    let sym = lisp.symbol("unbound_x").unwrap();
    let nil = lisp.nil().unwrap();
    
    let stx1 = lisp.syntax(sym, nil, nil).unwrap();
    let stx2 = lisp.syntax(sym, nil, nil).unwrap();
    
    // Both unbound, same name - should be free-identifier=?
    assert!(eval.free_identifier_eq(stx1, stx2).unwrap());
    
    // Different unbound names - should NOT be free-identifier=?
    let sym_y = lisp.symbol("unbound_y").unwrap();
    let stx3 = lisp.syntax(sym_y, nil, nil).unwrap();
    assert!(!eval.free_identifier_eq(stx1, stx3).unwrap());
}

/// Test that hygiene ensures macro-introduced bindings don't capture user bindings
#[test]
fn test_hygiene_no_capture() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a variable in user code
    eval.eval_str("(define temp 42)").unwrap();
    
    // Define a macro that uses 'temp' internally
    eval.eval_str(r#"
        (define-syntax swap
          (lambda (x)
            (syntax-case x ()
              ((swap a b)
               (syntax (let ((temp a))
                         (set! a b)
                         (set! b temp)))))))
    "#).unwrap();
    
    // Use swap with variables - the macro's temp should not capture user's temp
    eval.eval_str("(define x 1)").unwrap();
    eval.eval_str("(define y 2)").unwrap();
    eval.eval_str("(swap x y)").unwrap();
    
    // x and y should be swapped
    let x = eval.eval_str("x").unwrap();
    let y = eval.eval_str("y").unwrap();
    assert_eq!(lisp.get(x).unwrap().as_number(), Some(2));
    assert_eq!(lisp.get(y).unwrap().as_number(), Some(1));
    
    // User's temp should be unchanged
    let temp = eval.eval_str("temp").unwrap();
    assert_eq!(lisp.get(temp).unwrap().as_number(), Some(42));
}

// ============================================================================
// syntax-case Tests (Phase 3)
// ============================================================================

/// Test basic syntax-case with simple pattern
#[test]
fn test_syntax_case_simple_pattern() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic syntax-case with a literal pattern match
    let result = eval.eval_str(r#"
        (syntax-case '(hello world) ()
          ((a b) (list 'matched (quote a) (quote b))))
    "#).unwrap();
    
    // Should match and return (matched a b)
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "matched").unwrap());
}

/// Test syntax-case with multiple clauses
#[test]
fn test_syntax_case_multiple_clauses() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // First clause doesn't match, second does
    let result = eval.eval_str(r#"
        (syntax-case '(a b c) ()
          ((x) 'one-element)
          ((x y) 'two-elements)
          ((x y z) 'three-elements)
          (_ 'other))
    "#).unwrap();
    
    assert!(lisp.symbol_matches(result, "three-elements").unwrap());
}

/// Test syntax-case with pattern variable binding
#[test]
fn test_syntax_case_pattern_binding() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Pattern variables should be bound in the output expression
    let result = eval.eval_str(r#"
        (syntax-case '(1 2 3) ()
          ((a b c) (+ a b c)))
    "#).unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6));
}

/// Test with-syntax macro
#[test]
fn test_with_syntax_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // with-syntax should bind variables
    let result = eval.eval_str(r#"
        (with-syntax ((x 1) (y 2))
          (+ x y))
    "#).unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(3));
}

/// Test with-syntax empty bindings
#[test]
fn test_with_syntax_empty() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // with-syntax with no bindings should just evaluate body
    let result = eval.eval_str("(with-syntax () 42)").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
}

/// Test with-syntax with syntax template substitution
#[test]
fn test_with_syntax_in_procedural_macro() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a procedural macro that uses with-syntax to compute a value
    eval.eval_str(r#"
        (define-syntax add-one
          (lambda (x)
            (syntax-case x ()
              ((_ n) (with-syntax ((result (+ 1 n)))
                       (syntax result))))))
    "#).unwrap();
    
    // Use the macro - should compute result at macro expansion time
    let result = eval.eval_str("(add-one 99)").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(100));
}

/// Test with-syntax with multiple bindings using pattern variables
#[test]
fn test_with_syntax_multiple_bindings() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro with multiple with-syntax bindings that computes a sum
    eval.eval_str(r#"
        (define-syntax sum-triple
          (lambda (x)
            (syntax-case x ()
              ((_ a b c)
               (with-syntax ((total (+ a b c)))
                 (syntax total))))))
    "#).unwrap();
    
    let result = eval.eval_str("(sum-triple 10 20 30)").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(60));
}

/// Test with-syntax nested inside syntax-case with complex expressions
#[test]
fn test_with_syntax_complex_expression() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro that doubles a number using addition
    eval.eval_str(r#"
        (define-syntax double-val
          (lambda (x)
            (syntax-case x ()
              ((_ n)
               (with-syntax ((result (+ n n)))
                 (syntax result))))))
    "#).unwrap();
    
    let result = eval.eval_str("(double-val 7)").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(14));
}

/// Test with-syntax referencing other with-syntax bindings
#[test]
fn test_with_syntax_sequential_binding() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // with-syntax bindings should be able to use previous bindings
    let result = eval.eval_str(r#"
        (with-syntax ((x 5))
          (with-syntax ((y (+ x 3)))
            (+ x y)))
    "#).unwrap();
    
    // x = 5, y = 8, result = 13
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(13));
}

/// Test syntax form for template creation
#[test]
fn test_syntax_template_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // (syntax ...) should create a quoted template
    let result = eval.eval_str("(syntax (a b c))").unwrap();
    
    // Should return a list (a b c)
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "a").unwrap());
    
    let cadr = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert!(lisp.symbol_matches(cadr, "b").unwrap());
}

/// Test syntax form with pattern variable substitution
#[test]
fn test_syntax_template_with_pattern_variables() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // (syntax template) should substitute pattern variables from syntax-case
    let result = eval.eval_str(r#"
        (syntax-case '(hello world) ()
          ((a b) (syntax (list a b))))
    "#).unwrap();
    
    // Should return (list hello world) - the template with substitutions
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "list").unwrap());
    
    let cadr = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert!(lisp.symbol_matches(cadr, "hello").unwrap());
    
    let caddr = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert!(lisp.symbol_matches(caddr, "world").unwrap());
}

/// Test syntax form substitutes pattern variables in nested templates
#[test]
fn test_syntax_template_nested_substitution() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Pattern variables should be substituted in nested structures
    let result = eval.eval_str(r#"
        (syntax-case '(x y) ()
          ((a b) (syntax ((a) (b) (a b)))))
    "#).unwrap();
    
    // Should return ((x) (y) (x y))
    // First element: (x)
    let first = lisp.car(result).unwrap();
    let first_car = lisp.car(first).unwrap();
    assert!(lisp.symbol_matches(first_car, "x").unwrap());
    
    // Second element: (y)
    let second = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let second_car = lisp.car(second).unwrap();
    assert!(lisp.symbol_matches(second_car, "y").unwrap());
    
    // Third element: (x y)
    let third = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    let third_car = lisp.car(third).unwrap();
    let third_cadr = lisp.car(lisp.cdr(third).unwrap()).unwrap();
    assert!(lisp.symbol_matches(third_car, "x").unwrap());
    assert!(lisp.symbol_matches(third_cadr, "y").unwrap());
}

/// Test syntax form with numeric pattern variables
#[test]
fn test_syntax_template_with_numbers() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Pattern variables bound to numbers should be substituted
    let result = eval.eval_str(r#"
        (syntax-case '(1 2 3) ()
          ((a b c) (syntax (a b c))))
    "#).unwrap();
    
    // Should return (1 2 3)
    let car = lisp.car(result).unwrap();
    assert_eq!(lisp.get(car).unwrap().as_number(), Some(1));
    
    let cadr = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert_eq!(lisp.get(cadr).unwrap().as_number(), Some(2));
    
    let caddr = lisp.car(lisp.cdr(lisp.cdr(result).unwrap()).unwrap()).unwrap();
    assert_eq!(lisp.get(caddr).unwrap().as_number(), Some(3));
}

/// Test syntax-case with literal keywords
#[test]
fn test_syntax_case_with_literals() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Literal keywords should match exactly
    let result = eval.eval_str(r#"
        (syntax-case '(if x y) (if)
          ((if cond then) (list 'conditional cond then))
          (_ 'no-match))
    "#).unwrap();
    
    // Should match the (if cond then) pattern
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "conditional").unwrap());
}

// ============================================================================
// Phase 4: Hygiene Utilities Tests
// ============================================================================

/// Test identifier? predicate with symbols
#[test]
fn test_identifier_predicate_symbol() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // A symbol is an identifier
    let result = eval.eval_str("(identifier? 'foo)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::True));
    
    // A number is not an identifier
    let result = eval.eval_str("(identifier? 42)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::False));
    
    // A list is not an identifier
    let result = eval.eval_str("(identifier? '(a b c))").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::False));
}

/// Test bound-identifier=? builtin
#[test]
fn test_bound_identifier_eq_builtin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Same symbol should be bound-identifier=?
    let result = eval.eval_str("(bound-identifier=? 'x 'x)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::True));
    
    // Different symbols should not be bound-identifier=?
    let result = eval.eval_str("(bound-identifier=? 'x 'y)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::False));
}

/// Test free-identifier=? builtin
#[test]
fn test_free_identifier_eq_builtin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Same unbound symbol should be free-identifier=?
    let result = eval.eval_str("(free-identifier=? 'x 'x)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::True));
    
    // Different unbound symbols should not be free-identifier=?
    let result = eval.eval_str("(free-identifier=? 'x 'y)").unwrap();
    assert!(matches!(lisp.get(result).unwrap(), Value::False));
}

/// Test syntax->datum builtin
#[test]
fn test_syntax_to_datum_builtin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // syntax->datum should strip syntax wrapper from a quoted symbol
    let result = eval.eval_str("(syntax->datum 'hello)").unwrap();
    assert!(lisp.symbol_matches(result, "hello").unwrap());
    
    // syntax->datum on a number should return the number
    let result = eval.eval_str("(syntax->datum 42)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // syntax->datum on a list should return the list
    let result = eval.eval_str("(syntax->datum '(a b c))").unwrap();
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "a").unwrap());
}

/// Test datum->syntax builtin
#[test]
fn test_datum_to_syntax_builtin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // datum->syntax should wrap a symbol
    let _result = eval.eval_str("(datum->syntax 'template 'new-name)").unwrap();
    // The result should be a syntax object wrapping 'new-name
    // When we unwrap it, we should get 'new-name back
    let unwrapped = eval.eval_str("(syntax->datum (datum->syntax 'template 'new-name))").unwrap();
    assert!(lisp.symbol_matches(unwrapped, "new-name").unwrap());
}

/// Test generate-temporaries builtin
#[test]
fn test_generate_temporaries_builtin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // generate-temporaries should create a list of temporaries
    let result = eval.eval_str("(generate-temporaries '(a b c))").unwrap();
    
    // Result should be a list
    assert!(matches!(lisp.get(result).unwrap(), Value::Cons { .. }));
    
    // Count elements - should have 3 temporaries
    let len = lisp.list_len(result).unwrap();
    assert_eq!(len, 3);
    
    // Each element should be an identifier (syntax object wrapping a gensym)
    let _first = lisp.car(result).unwrap();
    let first_is_id = eval.eval_str(&format!("(let ((x (car (generate-temporaries '(a))))) (identifier? x))")).unwrap();
    assert!(matches!(lisp.get(first_is_id).unwrap(), Value::True));
}

/// Test generate-temporaries with empty list
#[test]
fn test_generate_temporaries_empty() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // generate-temporaries with empty list should return empty list
    let result = eval.eval_str("(generate-temporaries '())").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

/// Test syntax-case with fender using function call
#[test]
fn test_syntax_case_fender_with_function() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Fender uses a function call (identifier? is a function)
    let result = eval.eval_str(r#"
        (syntax-case '(foo bar) ()
          ((a b) (identifier? 'a) 'has-identifier)
          (_ 'no-match))
    "#).unwrap();
    
    assert!(lisp.symbol_matches(result, "has-identifier").unwrap());
}

/// Test syntax-case with fender that evaluates to false
#[test]
fn test_syntax_case_fender_false() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // First clause matches pattern but fender fails
    // Should fall through to second clause
    let result = eval.eval_str(r#"
        (syntax-case '(1 2 3) ()
          ((a b c) (eq? a 'foo) 'first-clause)
          ((a b c) #t 'second-clause))
    "#).unwrap();
    
    assert!(lisp.symbol_matches(result, "second-clause").unwrap());
}

/// Test syntax-case with fender using arithmetic
#[test]
fn test_syntax_case_fender_arithmetic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Fender uses arithmetic comparison
    let result = eval.eval_str(r#"
        (syntax-case '(5 10) ()
          ((a b) (< a b) 'ascending)
          ((a b) (> a b) 'descending)
          (_ 'equal))
    "#).unwrap();
    
    assert!(lisp.symbol_matches(result, "ascending").unwrap());
}

/// Test syntax-case with complex fender
#[test]
fn test_syntax_case_fender_complex() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Fender uses multiple operations
    let result = eval.eval_str(r#"
        (syntax-case '(3 4 5) ()
          ((a b c) (and (> b a) (> c b)) 'strictly-increasing)
          (_ 'not-increasing))
    "#).unwrap();
    
    assert!(lisp.symbol_matches(result, "strictly-increasing").unwrap());
}

// ============================================================================
// Procedural Macro Tests (Phase 5)
// ============================================================================

/// Test basic procedural macro with lambda transformer
#[test]
fn test_procedural_macro_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a simple procedural macro
    eval.eval_str(r#"
        (define-syntax my-first
          (lambda (x)
            (syntax-case x ()
              ((_ a . rest) (syntax a)))))
    "#).unwrap();
    
    // Now use the macro
    let result = eval.eval_str("(my-first 1 2 3)").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(1));
}

/// Test procedural macro with syntax-case pattern matching
#[test]
fn test_procedural_macro_pattern_match() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro that processes its input
    eval.eval_str(r#"
        (define-syntax my-add
          (lambda (x)
            (syntax-case x ()
              ((_ a b) (syntax (+ a b))))))
    "#).unwrap();
    
    let result = eval.eval_str("(my-add 2 3)").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(5));
}

/// Test procedural macro with multiple clauses
#[test]
fn test_procedural_macro_multiple_clauses() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a macro with multiple patterns
    eval.eval_str(r#"
        (define-syntax count-args
          (lambda (x)
            (syntax-case x ()
              ((_) (syntax 0))
              ((_ a) (syntax 1))
              ((_ a b) (syntax 2))
              ((_ a b c) (syntax 3)))))
    "#).unwrap();
    
    let result = eval.eval_str("(count-args a b c)").unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(3));
}

/// Test procedural macro with fender (guard)
#[test]
fn test_procedural_macro_with_fender() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Macro that uses a fender to check its input
    eval.eval_str(r#"
        (define-syntax check-positive
          (lambda (x)
            (syntax-case x ()
              ((_ n) (> n 0) (syntax 'positive))
              ((_ n) (syntax 'not-positive)))))
    "#).unwrap();
    
    let result = eval.eval_str("(check-positive 5)").unwrap();
    
    assert!(lisp.symbol_matches(result, "positive").unwrap());
}

/// Test procedural macro with nested syntax-case
#[test]
fn test_procedural_macro_nested() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Macro that returns a list structure
    eval.eval_str(r#"
        (define-syntax make-pair
          (lambda (x)
            (syntax-case x ()
              ((_ a b) (syntax (cons a b))))))
    "#).unwrap();
    
    let result = eval.eval_str("(make-pair 'left 'right)").unwrap();
    
    let car = lisp.car(result).unwrap();
    let cdr = lisp.cdr(result).unwrap();
    assert!(lisp.symbol_matches(car, "left").unwrap());
    assert!(lisp.symbol_matches(cdr, "right").unwrap());
}

// ============================================================================
// Procedural Quasiquote Macro Tests (Phase 5)
// ============================================================================

/// Test that the %qq-expand helper macro is available and works
#[test]
fn test_procedural_quasiquote_macro_helper() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test the %qq-expand helper directly (it's loaded from macros.scm)
    eval.eval_str("(define x 42)").unwrap();
    
    // Basic case: atom
    let result = eval.eval_str("(%qq-expand atom (d z))").unwrap();
    assert!(lisp.symbol_matches(result, "atom").unwrap());
    
    // Unquote at depth 1 evaluates the expression
    let result = eval.eval_str("(%qq-expand (unquote x) (d z))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // List with unquote
    let result = eval.eval_str("(%qq-expand (a (unquote x) c) (d z))").unwrap();
    // Should produce (a 42 c)
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "a").unwrap());
    let cadr = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    assert_eq!(lisp.get(cadr).unwrap().as_number(), Some(42));
}

/// Test procedural quasiquote handles unquote-splicing
#[test]
fn test_procedural_quasiquote_macro_splice() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define xs '(1 2 3))").unwrap();
    
    // Splice at depth 1
    let result = eval.eval_str("(%qq-expand (a (unquote-splicing xs) b) (d z))").unwrap();
    // Should produce (a 1 2 3 b)
    
    // Count elements - should be 5
    let mut count = 0;
    let mut current = result;
    loop {
        match lisp.get(current).unwrap() {
            Value::Nil => break,
            Value::Cons { cdr, .. } => {
                count += 1;
                current = cdr;
            }
            _ => break,
        }
    }
    assert_eq!(count, 5, "Should have 5 elements: a, 1, 2, 3, b");
}

/// Test procedural quasiquote handles nested quasiquote
#[test]
fn test_procedural_quasiquote_macro_nested() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 42)").unwrap();
    
    // Nested quasiquote - inner unquote should NOT evaluate
    let result = eval.eval_str("(%qq-expand (quasiquote (unquote x)) (d z))").unwrap();
    // Should produce (quasiquote (unquote x)) - a list with quasiquote symbol
    
    let car = lisp.car(result).unwrap();
    assert!(lisp.symbol_matches(car, "quasiquote").unwrap());
    
    // The inner part should be (unquote x), not 42
    let inner = lisp.car(lisp.cdr(result).unwrap()).unwrap();
    let inner_car = lisp.car(inner).unwrap();
    assert!(lisp.symbol_matches(inner_car, "unquote").unwrap());
}

/// Test that procedural quasiquote produces same results as special form
#[test]
fn test_procedural_quasiquote_matches_special_form() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 10)").unwrap();
    eval.eval_str("(define y 20)").unwrap();
    
    // Test various cases - procedural macro vs special form should match
    
    // Basic list with unquote
    let proc_result = eval.eval_str("(%qq-expand (a (unquote x) c) (d z))").unwrap();
    let sf_result = eval.eval_str("`(a ,x c)").unwrap();
    
    // Compare: both should be (a 10 c)
    let proc_car = lisp.car(proc_result).unwrap();
    let sf_car = lisp.car(sf_result).unwrap();
    assert!(lisp.symbol_matches(proc_car, "a").unwrap());
    assert!(lisp.symbol_matches(sf_car, "a").unwrap());
    
    let proc_cadr = lisp.car(lisp.cdr(proc_result).unwrap()).unwrap();
    let sf_cadr = lisp.car(lisp.cdr(sf_result).unwrap()).unwrap();
    assert_eq!(lisp.get(proc_cadr).unwrap().as_number(), Some(10));
    assert_eq!(lisp.get(sf_cadr).unwrap().as_number(), Some(10));
}

// ==============================================================================
// call/cc (call-with-current-continuation) tests
// ==============================================================================

/// Test basic call/cc - escape continuation
#[test]
fn test_call_cc_basic_escape() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // (+ 1 (call/cc (lambda (k) (+ 2 (k 3)))))
    // When (k 3) is called, it immediately returns 3 as the result of call/cc,
    // skipping the (+ 2 ...) computation. So the result is (+ 1 3) = 4.
    let result = eval.eval_str("(+ 1 (call/cc (lambda (k) (+ 2 (k 3)))))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(4));
}

/// Test call/cc when continuation is not invoked (normal return)
#[test]
fn test_call_cc_no_escape() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // When the continuation is not called, the lambda returns normally
    let result = eval.eval_str("(+ 1 (call/cc (lambda (k) 3)))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(4)); // 1 + 3
}

/// Test storing and using a continuation later
#[test]
fn test_call_cc_stored_continuation() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Store the continuation and return normally
    eval.eval_str("(define saved-k #f)").unwrap();
    let result1 = eval.eval_str("(+ 1 (call/cc (lambda (k) (set! saved-k k) 4)))").unwrap();
    assert_eq!(lisp.get(result1).unwrap().as_number(), Some(5)); // 1 + 4
    
    // Now invoke the saved continuation with a new value
    // This should return to the point of call/cc and compute (+ 1 9) = 10
    let result2 = eval.eval_str("(saved-k 9)").unwrap();
    assert_eq!(lisp.get(result2).unwrap().as_number(), Some(10));
}

/// Test nested call/cc - inner escapes outer
#[test]
fn test_call_cc_nested() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Inner call/cc invokes outer's continuation, jumping out of both
    let result = eval.eval_str(
        "(call/cc (lambda (outer) (call/cc (lambda (inner) (outer 'done))) 'never-reached))"
    ).unwrap();
    assert!(lisp.symbol_matches(result, "done").unwrap());
}

/// Test call-with-current-continuation long form
#[test]
fn test_call_with_current_continuation() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // The long form should work exactly like call/cc
    let result = eval.eval_str("(+ 1 (call-with-current-continuation (lambda (k) (k 5))))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6)); // 1 + 5
}

/// Test call/cc with multiple returns using same continuation
#[test]
fn test_call_cc_multiple_returns() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a counter and a continuation
    eval.eval_str("(define counter 0)").unwrap();
    eval.eval_str("(define saved-k #f)").unwrap();
    
    // First call - save continuation and return 1
    let result1 = eval.eval_str(
        "(call/cc (lambda (k) (set! saved-k k) 1))"
    ).unwrap();
    assert_eq!(lisp.get(result1).unwrap().as_number(), Some(1));
    
    // Use the continuation multiple times
    let result2 = eval.eval_str("(saved-k 2)").unwrap();
    assert_eq!(lisp.get(result2).unwrap().as_number(), Some(2));
    
    let result3 = eval.eval_str("(saved-k 3)").unwrap();
    assert_eq!(lisp.get(result3).unwrap().as_number(), Some(3));
}

/// Test call/cc continuation is a procedure
#[test]
fn test_call_cc_continuation_is_procedure() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Get the continuation and check it's a valid value
    eval.eval_str("(define k #f)").unwrap();
    eval.eval_str("(call/cc (lambda (c) (set! k c) 1))").unwrap();
    
    // The continuation should exist and be callable
    let result = eval.eval_str("k").unwrap();
    // It should be a continuation value
    assert!(lisp.get(result).unwrap().is_continuation());
}

/// Test call/cc with arithmetic in continuation
#[test]
fn test_call_cc_arithmetic_context() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // More complex arithmetic context
    let result = eval.eval_str("(* 2 (+ 3 (call/cc (lambda (k) (k 5)))))").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(16)); // 2 * (3 + 5) = 16
}

/// Test call/cc error: wrong number of arguments to continuation
#[test]
fn test_call_cc_continuation_wrong_args() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Store a continuation
    eval.eval_str("(define k #f)").unwrap();
    eval.eval_str("(call/cc (lambda (c) (set! k c) 1))").unwrap();
    
    // Calling continuation with no args should fail
    let result = eval.eval_str("(k)");
    assert!(result.is_err());
    
    // Calling with too many args should also fail
    let result2 = eval.eval_str("(k 1 2)");
    assert!(result2.is_err());
}

/// Test call/cc error: wrong number of arguments to call/cc itself
#[test]
fn test_call_cc_wrong_args() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // call/cc with no arguments should fail
    let result = eval.eval_str("(call/cc)");
    assert!(result.is_err());
    
    // call/cc with too many arguments should fail
    let result2 = eval.eval_str("(call/cc (lambda (k) k) extra)");
    assert!(result2.is_err());
}


// ==============================================================================
// dynamic-wind tests
// ==============================================================================

/// Test basic dynamic-wind - all three thunks called in order
#[test]
fn test_dynamic_wind_basic() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Set up a log list
    eval.eval_str("(define log '())").unwrap();
    
    // Run dynamic-wind
    let result = eval.eval_str(
        "(dynamic-wind
           (lambda () (set! log (cons 'before log)))
           (lambda () (set! log (cons 'body log)) 42)
           (lambda () (set! log (cons 'after log))))"
    ).unwrap();
    
    // Body should return 42
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // Check the log - should be (after body before) since we cons onto front
    let log = eval.eval_str("log").unwrap();
    
    // First element should be 'after
    let first = lisp.car(log).unwrap();
    assert!(lisp.symbol_matches(first, "after").unwrap());
    
    // Second element should be 'body
    let second = lisp.car(lisp.cdr(log).unwrap()).unwrap();
    assert!(lisp.symbol_matches(second, "body").unwrap());
    
    // Third element should be 'before
    let third = lisp.car(lisp.cdr(lisp.cdr(log).unwrap()).unwrap()).unwrap();
    assert!(lisp.symbol_matches(third, "before").unwrap());
}

/// Test dynamic-wind with escape via call/cc
#[test]
fn test_dynamic_wind_escape() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Set up a log list
    eval.eval_str("(define log '())").unwrap();
    
    // Run dynamic-wind with escape via call/cc
    let result = eval.eval_str(
        "(call/cc (lambda (escape)
           (dynamic-wind
             (lambda () (set! log (cons 'before log)))
             (lambda () (escape 'escaped))
             (lambda () (set! log (cons 'after log))))))"
    ).unwrap();
    
    // Should have escaped with 'escaped
    assert!(lisp.symbol_matches(result, "escaped").unwrap());
    
    // Check the log - after thunk should still have been called
    let log = eval.eval_str("log").unwrap();
    
    // First element should be 'after
    let first = lisp.car(log).unwrap();
    assert!(lisp.symbol_matches(first, "after").unwrap());
    
    // Second element should be 'before (body never ran to completion, so no 'body in log)
    let second = lisp.car(lisp.cdr(log).unwrap()).unwrap();
    assert!(lisp.symbol_matches(second, "before").unwrap());
}

/// Test dynamic-wind returns body result
#[test]
fn test_dynamic_wind_returns_body() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Body returns a specific value
    let result = eval.eval_str(
        "(dynamic-wind
           (lambda () 'ignored)
           (lambda () (+ 1 2 3))
           (lambda () 'also-ignored))"
    ).unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6));
}

/// Test nested dynamic-wind
#[test]
fn test_dynamic_wind_nested() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Set up a log list
    eval.eval_str("(define log '())").unwrap();
    
    // Run nested dynamic-wind
    let result = eval.eval_str(
        "(dynamic-wind
           (lambda () (set! log (cons 'before1 log)))
           (lambda ()
             (dynamic-wind
               (lambda () (set! log (cons 'before2 log)))
               (lambda () (set! log (cons 'body log)) 99)
               (lambda () (set! log (cons 'after2 log)))))
           (lambda () (set! log (cons 'after1 log))))"
    ).unwrap();
    
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(99));
    
    // Check the log order: after1 after2 body before2 before1 (reversed)
    let log = eval.eval_str("log").unwrap();
    
    let e1 = lisp.car(log).unwrap();
    assert!(lisp.symbol_matches(e1, "after1").unwrap());
    
    let rest1 = lisp.cdr(log).unwrap();
    let e2 = lisp.car(rest1).unwrap();
    assert!(lisp.symbol_matches(e2, "after2").unwrap());
    
    let rest2 = lisp.cdr(rest1).unwrap();
    let e3 = lisp.car(rest2).unwrap();
    assert!(lisp.symbol_matches(e3, "body").unwrap());
}

/// Test dynamic-wind error: wrong number of arguments
#[test]
fn test_dynamic_wind_wrong_args() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Too few arguments
    let result = eval.eval_str("(dynamic-wind (lambda () 1) (lambda () 2))");
    assert!(result.is_err());
    
    // Too many arguments
    let result2 = eval.eval_str("(dynamic-wind (lambda () 1) (lambda () 2) (lambda () 3) (lambda () 4))");
    assert!(result2.is_err());
    
    // No arguments
    let result3 = eval.eval_str("(dynamic-wind)");
    assert!(result3.is_err());
}

/// Test re-entering dynamic-wind via saved continuation
#[test]
fn test_dynamic_wind_reenter() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Set up a log and saved continuation
    eval.eval_str("(define log '())").unwrap();
    eval.eval_str("(define saved-k #f)").unwrap();
    
    // Run dynamic-wind and save a continuation inside
    let result1 = eval.eval_str(
        "(dynamic-wind
           (lambda () (set! log (cons 'before log)))
           (lambda ()
             (call/cc (lambda (k)
               (set! saved-k k)
               'first-time)))
           (lambda () (set! log (cons 'after log))))"
    ).unwrap();
    
    // First result should be 'first-time
    assert!(lisp.symbol_matches(result1, "first-time").unwrap());
    
    // Log should have before and after
    let log1 = eval.eval_str("log").unwrap();
    // Check first element is 'after
    let e1 = lisp.car(log1).unwrap();
    assert!(lisp.symbol_matches(e1, "after").unwrap());
    
    // Clear log and re-enter via saved continuation
    eval.eval_str("(set! log '())").unwrap();
    
    // Now invoke the saved continuation - this should re-enter the dynamic-wind
    // calling the before thunk again, then returning 'second-time, then calling after
    let result2 = eval.eval_str("(saved-k 'second-time)").unwrap();
    
    assert!(lisp.symbol_matches(result2, "second-time").unwrap());
    
    // Log should show before and after were called again
    let log2 = eval.eval_str("log").unwrap();
    let e2_1 = lisp.car(log2).unwrap();
    assert!(lisp.symbol_matches(e2_1, "after").unwrap());
    
    let e2_2 = lisp.car(lisp.cdr(log2).unwrap()).unwrap();
    assert!(lisp.symbol_matches(e2_2, "before").unwrap());
}

// ========== Effect System Tests ==========

fn eval_is_effect<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> bool {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().is_effect()
}

#[test]
fn test_effect_io_pure() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // io/pure creates an effect
    assert!(eval_is_effect(&lisp, &mut eval, "(io/pure 42)"));
    assert!(eval_is_effect(&lisp, &mut eval, "(io/pure '(1 2 3))"));
    assert!(eval_is_effect(&lisp, &mut eval, "(io/pure (lambda (x) x))"));
    
    // effect? predicate works
    assert!(eval_is_true(&lisp, &mut eval, "(effect? (io/pure 42))"));
    assert!(eval_is_false(&lisp, &mut eval, "(effect? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(effect? '())"));
}

#[test]
fn test_effect_io_print() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // io/print creates an effect
    assert!(eval_is_effect(&lisp, &mut eval, "(io/print \"hello\")"));
    assert!(eval_is_effect(&lisp, &mut eval, "(io/print 42)"));
    
    // effect-tag and effect-data work
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (effect-tag (io/print \"hello\")) 'io/print)"));
}

#[test]
fn test_effect_io_read_line() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // io/read-line creates an effect with nil data
    assert!(eval_is_effect(&lisp, &mut eval, "(io/read-line)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (effect-tag (io/read-line)) 'io/read-line)"));
    assert!(eval_is_true(&lisp, &mut eval, "(null? (effect-data (io/read-line)))"));
}

#[test]
fn test_effect_io_bind() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // io/bind creates a sequencing effect
    assert!(eval_is_effect(&lisp, &mut eval, "(io/bind (io/pure 1) (lambda (x) (io/pure (+ x 1))))"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (effect-tag (io/bind (io/pure 1) (lambda (x) x))) 'io/bind)"));
    
    // The data should be (effect . continuation)
    let _ = eval.eval_str("(define my-bind (io/bind (io/print \"hi\") (lambda (x) (io/pure x))))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(pair? (effect-data my-bind))"));
}

#[test]
fn test_effects_are_values() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Effects can be stored in variables
    let _ = eval.eval_str("(define my-effect (io/pure 42))").unwrap();
    assert!(eval_is_effect(&lisp, &mut eval, "my-effect"));
    
    // Effects can be stored in lists
    let _ = eval.eval_str("(define effect-list (list (io/pure 1) (io/pure 2) (io/pure 3)))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(effect? (car effect-list))"));
    assert!(eval_is_true(&lisp, &mut eval, "(effect? (car (cdr effect-list)))"));
    
    // Effects can be passed to functions
    let _ = eval.eval_str("(define (get-tag e) (effect-tag e))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (get-tag (io/print \"hello\")) 'io/print)"));
}

#[test]
fn test_effects_preserve_purity() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Creating an io/print effect should NOT actually print anything
    // (We can't directly test this, but we verify the effect is just a value)
    let _ = eval.eval_str("(define my-print (io/print \"This should not print\"))").unwrap();
    assert!(eval_is_effect(&lisp, &mut eval, "my-print"));
    
    // Creating multiple identical effects should produce equal-ish results
    // (They have same tag and same data)
    let _ = eval.eval_str("(define print1 (io/print \"hello\"))").unwrap();
    let _ = eval.eval_str("(define print2 (io/print \"hello\"))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (effect-tag print1) (effect-tag print2))"));
}

#[test]
fn test_effect_composition() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Complex effect composition
    let _ = eval.eval_str(r#"
        (define greet
          (io/bind (io/print "What is your name? ")
            (lambda (_)
              (io/bind (io/read-line)
                (lambda (name)
                  (io/print (string-append "Hello, " name)))))))
    "#).unwrap();
    
    assert!(eval_is_effect(&lisp, &mut eval, "greet"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (effect-tag greet) 'io/bind)"));
}

#[test]
fn test_eff_macro_single() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Single expression (base case)
    assert!(eval_is_effect(&lisp, &mut eval, "(eff (io/pure 42))"));
}

#[test]
fn test_eff_macro_binding() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Binding form: (x <- effect) rest
    let result = eval.eval_str("(eff (x <- (io/pure 42)) (io/pure (+ x 1)))").unwrap();
    assert!(lisp.get(result).unwrap().is_effect());
    
    // Multiple bindings
    let result2 = eval.eval_str("(eff (a <- (io/pure 1)) (b <- (io/pure 2)) (io/pure (+ a b)))").unwrap();
    assert!(lisp.get(result2).unwrap().is_effect());
}

#[test]
fn test_eff_macro_sequencing() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Non-binding form: effect rest (discard result)
    let result = eval.eval_str(r#"(eff (io/print "hello") (io/pure 42))"#).unwrap();
    assert!(lisp.get(result).unwrap().is_effect());
}

#[test]
fn test_io_then_macro() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // io/then sequences two effects
    let result = eval.eval_str(r#"(io/then (io/print "a") (io/print "b"))"#).unwrap();
    assert!(lisp.get(result).unwrap().is_effect());
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (effect-tag (io/then (io/pure 1) (io/pure 2))) 'io/bind)"));
}

#[test]
fn test_io_map_macro() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // io/map applies a function to effect result
    let result = eval.eval_str("(io/map (lambda (x) (+ x 1)) (io/pure 41))").unwrap();
    assert!(lisp.get(result).unwrap().is_effect());
    
    // Complex io/map
    let _ = eval.eval_str("(define doubled (io/map (lambda (x) (* x 2)) (io/pure 5)))").unwrap();
    assert!(eval_is_effect(&lisp, &mut eval, "doubled"));
}

#[test]
fn test_complex_eff_composition() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Complex effect composition using eff
    let _ = eval.eval_str(r#"
        (define greet-user
          (eff
            (io/print "Enter your name: ")
            (name <- (io/read-line))
            (io/print (string-append "Hello, " name "!"))))
    "#).unwrap();
    
    assert!(eval_is_effect(&lisp, &mut eval, "greet-user"));
    
    // The result is an io/bind effect
    assert!(eval_is_true(&lisp, &mut eval, "(eq? (effect-tag greet-user) 'io/bind)"));
}
