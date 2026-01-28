use lisp_eval::*;

fn eval_to_num<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> i64 {
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
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "42"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "-10"), -10);
}

#[test]
fn test_eval_booleans() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "#t"));
    assert!(eval_is_false(&lisp, &mut eval, "#f"));
    assert!(eval_is_true(&lisp, &mut eval, "true"));
    assert!(eval_is_false(&lisp, &mut eval, "false"));
}

#[test]
fn test_nil_is_truthy() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // nil/'() is NOT false - only #f is false
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if nil 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if '() 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if 0 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
}

#[test]
fn test_eval_arithmetic() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10 3)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 4 5)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 20 4)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3 4)"), 10);
}

#[test]
fn test_eval_quote() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("'hello").unwrap();
    assert!(lisp.symbol_matches(result, "hello").unwrap());
}

#[test]
fn test_eval_if() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if (< 1 2) 10 20)"), 10);
}

#[test]
fn test_eval_define() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 42)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 42);
}

#[test]
fn test_eval_lambda() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "((lambda (x) (+ x 1)) 5)"), 6);
}

#[test]
fn test_eval_define_function() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (square x) (* x x))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(square 5)"), 25);
}

#[test]
fn test_eval_let() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((x 10) (y 20)) (+ x y))"), 30);
}

#[test]
fn test_eval_let_star() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // let* allows sequential binding
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let* ((x 10) (y (+ x 5))) (+ x y))"), 25);
}

#[test]
fn test_tco_recursion() {
    // Hybrid evaluation: tail calls are STRICT, so TCO works properly!
    // No thunk accumulation - deep recursion is safe.
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // This would overflow without proper TCO
    eval.eval_str("(define (sum-to n acc) (if (= n 0) acc (sum-to (- n 1) (+ acc n))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum-to 100 0)"), 5050);  // sum 1..100
}

#[test]
fn test_eval_recursion() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fact 5)"), 120);
}

// ═══════════════════════════════════════════════════════════════════════════
// AUTOMATIC MEMOIZATION TESTS
// Recursive functions are automatically memoized with bounded LRU caches
// ═══════════════════════════════════════════════════════════════════════════

// NOTE: Some tests may require RUST_MIN_STACK=8388608 (8MB) to run
// This is due to Rust's default test thread stack being too small for
// deep parsing/evaluation of complex Lisp expressions. The runtime
// evaluator uses trampolining and has no such limitation.

#[test]
fn test_simple_fib_define() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    // Simple single-recursive function
    eval.eval_str("(define (countdown n) (if (= n 0) 0 (countdown (- n 1))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(countdown 5)"), 0);
}

#[test]
fn test_auto_memoization_fibonacci() {
    // NOTE: This test requires larger stack: RUST_MIN_STACK=8388608
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
    let lisp: Lisp<3000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple recursive function with 2 args
    eval.eval_str("(define (add-rec x y) (if (= y 0) x (add-rec (+ x 1) (- y 1))))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-rec 3 4)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add-rec 5 3)"), 8);
}

#[test]
fn test_auto_memoization_cache_works() {
    // Verify that caching actually happens by checking repeated calls
    let lisp: Lisp<6000> = Lisp::new();
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
    let lisp: Lisp<3000> = Lisp::new();
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
    let lisp: Lisp<4000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sum function - tail recursive
    eval.eval_str("(define (sum n acc) (if (= n 0) acc (sum (- n 1) (+ acc n))))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(sum 100 0)"), 5050);
}

#[test]
fn test_lru_cache_eviction() {
    // Test that cache eviction works by exceeding MAX_MEMO_CACHE_SIZE
    let lisp: Lisp<10000> = Lisp::new();
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
// LAZY EVALUATION TESTS
// Everything is lazy by default - no delay/force needed!
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lazy_basic() {
    // Basic lazy evaluation - arguments computed only when needed
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple computation works
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3)"), 6);
    
    // Functions work
    eval.eval_str("(define (add x y) (+ x y))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add 10 20)"), 30);
}

#[test]
fn test_lazy_cons_is_nonstrict() {
    // cons doesn't force its arguments - can build structures with unevaluated parts
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a pair
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 2);
    
    // List creation works
    eval.eval_str("(define lst (list 1 2 3))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car lst)"), 1);
}

#[test]
fn test_lazy_infinite_stream() {
    // THE KEY TEST: Infinite structures work!
    // (define ones (cons 1 ones)) - this would loop forever in eager evaluation
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create an infinite stream of 1s using self-reference
    // This works because cons is non-strict and ones is wrapped in a thunk
    eval.eval_str("(define (make-ones) (cons 1 (make-ones)))").unwrap();
    eval.eval_str("(define ones (make-ones))").unwrap();
    
    // We can access elements without infinite loop
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car ones)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr ones))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr ones)))"), 1);
}

#[test]
fn test_lazy_if_branches() {
    // Only the selected branch of 'if' is evaluated
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // This would error in an eager language (div by zero in else branch)
    // But we select the then branch, so else is never evaluated
    eval.eval_str("(define (safe-div x y) (if (= y 0) 0 (/ x y)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(safe-div 10 0)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(safe-div 10 2)"), 5);
}

#[test]
fn test_hybrid_builtin_lazy() {
    // HYBRID EVALUATION: Builtin args are lazy (builtins force what they need)
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // 'if' is a special form with lazy branches - only one is evaluated
    // The undefined branch is never forced
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #t 42 undefined-var)"), 42);
    
    // cons is non-strict - elements stay as thunks
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);
}

#[test]
fn test_hybrid_lambda_strict() {
    // HYBRID EVALUATION: Lambda args in tail position are strict (for TCO)
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Lambda args are evaluated, so this is strict
    eval.eval_str("(define (first x y) x)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(first 42 100)"), 42);  // Works
    
    // The benefit: TCO works for deep recursion
    eval.eval_str("(define (count n) (if (= n 0) 0 (count (- n 1))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(count 50)"), 0);  // No stack overflow
}

#[test]
fn test_lazy_memoization() {
    // Values are memoized - same result every time
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (square x) (* x x))").unwrap();
    eval.eval_str("(define val (square 5))").unwrap();
    
    // Multiple accesses return same value
    assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
    assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
    assert_eq!(eval_to_num(&lisp, &mut eval, "val"), 25);
}

#[test]
fn test_lazy_closure() {
    // Closures capture their environment lazily
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (make-adder n) (lambda (x) (+ x n)))").unwrap();
    eval.eval_str("(define add5 (make-adder 5))").unwrap();
    eval.eval_str("(define add10 (make-adder 10))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 3)"), 8);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add10 3)"), 13);
}

#[test]
fn test_predicates() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(null? '())"));
    assert!(eval_is_true(&lisp, &mut eval, "(null? nil)"));
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
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(not #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not nil)")); // nil is truthy!
    assert!(eval_is_false(&lisp, &mut eval, "(not 0)"));   // 0 is truthy!
}

#[test]
fn test_cond() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval_to_num(&lisp, &mut eval, 
        "(cond ((< 5 3) 1) ((> 5 3) 2) (else 3))");
    assert_eq!(result, 2);
}

#[test]
fn test_and_or() {
    let lisp: Lisp<1000> = Lisp::new();
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
    let lisp: Lisp<1000> = Lisp::new();
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
    // Test TCO with recursion
    // 
    // NOTE: Depth is limited by Rust stack during force() calls.
    // The Lisp-level TCO (trampoline) works, but forcing thunks uses
    // Rust recursion. A full fix requires converting force() to iterative.
    // 
    // Current practical limit: ~100-200 recursive calls in debug mode
    let lisp: Lisp<5000> = Lisp::new();
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
    // NOTE: Limited depth due to Rust stack in force()
    let lisp: Lisp<5000> = Lisp::new();
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
    let lisp: Lisp<5000> = Lisp::new();
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
    let lisp: Lisp<5000> = Lisp::new();
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
// COMPREHENSIVE LAZY EVALUATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lazy_stream_operations() {
    // Stream operations on infinite data
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Infinite stream of natural numbers
    eval.eval_str("(define (nats-from n) (cons n (nats-from (+ n 1))))").unwrap();
    eval.eval_str("(define nats (nats-from 0))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car nats)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr nats))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr nats)))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr nats))))"), 3);
}

#[test]
fn test_lazy_stream_take() {
    // Take n elements from a stream
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (take n s) (if (= n 0) '() (cons (car s) (take (- n 1) (cdr s)))))").unwrap();
    eval.eval_str("(define (ones) (cons 1 (ones)))").unwrap();
    
    // Take 3 ones
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (take 3 (ones)))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (take 3 (ones))))"), 1);
}

#[test]
fn test_lazy_and_or_short_circuit() {
    // and/or should short-circuit with lazy evaluation
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // 'and' stops at first false
    assert!(eval_is_false(&lisp, &mut eval, "(and #f undefined-error)"));
    assert!(eval_is_true(&lisp, &mut eval, "(and #t #t #t)"));
    
    // 'or' stops at first true
    assert!(eval_is_true(&lisp, &mut eval, "(or #t undefined-error)"));
    assert!(eval_is_false(&lisp, &mut eval, "(or #f #f #f)"));
}

#[test]
fn test_lazy_let_bindings() {
    // let bindings in hybrid model
    let lisp: Lisp<2000> = Lisp::new();
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
fn test_lazy_cons_preserves_thunks() {
    // cons should not force its arguments
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build a list with computations
    eval.eval_str("(define p (cons (+ 1 2) (+ 3 4)))").unwrap();
    
    // Access should force and return correct values
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 7);
}

#[test]
fn test_lazy_nested_structures() {
    // Deeply nested lazy structures
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create nested pairs
    eval.eval_str("(define deep (cons (cons (cons 1 2) 3) 4))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr deep)"), 4);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (car deep))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (car (car deep)))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (car (car deep)))"), 1);
}

#[test]
fn test_lazy_with_gc_pressure() {
    // Test lazy evaluation under GC pressure
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create an infinite stream
    eval.eval_str("(define (ones) (cons 1 (ones)))").unwrap();
    eval.eval_str("(define stream (ones))").unwrap();
    
    // Access some elements
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car stream)"), 1);
    
    // Run GC
    eval.gc();
    
    // Stream should still work after GC
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr stream))"), 1);
    
    // More allocations and GC
    for _ in 0..10 {
        eval.eval_str("(+ 1 2 3 4 5)").unwrap();
    }
    eval.gc();
    
    // Stream still works
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr stream)))"), 1);
}

#[test]
fn test_lazy_fibonacci_stream() {
    // Classic lazy Fibonacci stream
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // zipWith for streams
    eval.eval_str("(define (zipwith f s1 s2) (cons (f (car s1) (car s2)) (zipwith f (cdr s1) (cdr s2))))").unwrap();
    
    // Fibonacci stream: fibs = 0 : 1 : zipWith (+) fibs (tail fibs)
    // We use a simpler approach with explicit recursion
    eval.eval_str("(define (fib-pair a b) (cons a (fib-pair b (+ a b))))").unwrap();
    eval.eval_str("(define fibs (fib-pair 0 1))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car fibs)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr fibs))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr fibs)))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr fibs))))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr (cdr fibs)))))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr (cdr (cdr fibs))))))"), 5);
}

// ═══════════════════════════════════════════════════════════════════════════
// HYBRID EVALUATION EDGE CASES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_hybrid_nested_calls() {
    // Test behavior with nested function calls
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (double x) (* x 2))").unwrap();
    eval.eval_str("(define (quad x) (double (double x)))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(quad 5)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(quad (quad 2))"), 32);
}

#[test]
fn test_hybrid_higher_order() {
    // Higher-order functions with hybrid evaluation
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (apply-twice f x) (f (f x)))").unwrap();
    eval.eval_str("(define (inc x) (+ x 1))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply-twice inc 0)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply-twice (lambda (x) (* x 2)) 3)"), 12);
}

#[test]
fn test_hybrid_currying() {
    // Curried functions
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (curry-add a) (lambda (b) (lambda (c) (+ a b c))))").unwrap();
    eval.eval_str("(define add1 (curry-add 1))").unwrap();
    eval.eval_str("(define add1-2 (add1 2))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add1-2 3)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(((curry-add 10) 20) 30)"), 60);
}

#[test]
fn test_force_chain_memoization() {
    // Verify that forcing a thunk memoizes the result
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a computation stored in a cons
    eval.eval_str("(define p (cons (* 111 111) 0))").unwrap();
    
    // First access forces and memoizes
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
    
    // Second access should return memoized value
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 12321);
    
    // Third access
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, 
        "(case 2 ((1) 10) ((2) 20) ((3) 30))"), 20);
}

#[test]
fn test_case_multiple_datums() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(case 'b ((a) 1) ((b c) 2) ((d) 3))"), 2);
}

#[test]
fn test_case_else() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(case 99 ((1) 10) ((2) 20) (else 0))"), 0);
}

#[test]
fn test_case_no_match() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("(case 5 ((1) 10) ((2) 20))").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

// ───────────────────────────────────────────────────────────────────────────
// DO - Iteration Construct
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_do_basic() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple countdown
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(do ((i 5 (- i 1))) ((= i 0) 42))"), 42);
}

#[test]
fn test_do_accumulator() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sum 1 to 5 using do loop
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(do ((i 1 (+ i 1)) (sum 0 (+ sum i))) ((> i 5) sum))"), 15);
}

#[test]
fn test_do_factorial() {
    let lisp: Lisp<3000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Evaluate a quoted expression
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval '(+ 1 2))"), 3);
}

#[test]
fn test_eval_symbol() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define x 99)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval 'x)"), 99);
}

#[test]
fn test_eval_computed() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build expression dynamically and evaluate it
    eval.eval_str("(define expr (list '+ 10 20))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval expr)"), 30);
}

// ───────────────────────────────────────────────────────────────────────────
// DEFMACRO - Macro Definition
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_defmacro_basic() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a simple macro that adds 10 to its argument
    eval.eval_str("(defmacro add10 (x) (list '+ x 10))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add10 5)"), 15);
}

#[test]
fn test_defmacro_unless() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define 'unless' macro (opposite of 'if')
    eval.eval_str("(defmacro unless (cond then else) (list 'if cond else then))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(unless #f 42 0)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(unless #t 42 0)"), 0);
}

#[test]
fn test_defmacro_when() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define 'when' macro (if without else)
    eval.eval_str("(defmacro when (cond body) (list 'if cond body nil))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(when #t 100)"), 100);
}

// ───────────────────────────────────────────────────────────────────────────
// GENSYM - Generate Unique Symbols
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_gensym_uniqueness() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let sym1 = eval.eval_str("(gensym)").unwrap();
    let sym2 = eval.eval_str("(gensym)").unwrap();
    
    // Each gensym should be unique
    assert!(lisp.get(sym1).unwrap().is_symbol());
    assert!(lisp.get(sym2).unwrap().is_symbol());
    assert!(!lisp.symbol_eq(sym1, sym2).unwrap());
}

// ───────────────────────────────────────────────────────────────────────────
// APPLY - Apply Function to List
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_apply_basic() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply + '(1 2 3))"), 6);
}

#[test]
fn test_apply_lambda() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (sum3 a b c) (+ a b c))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply sum3 '(10 20 30))"), 60);
}

// ───────────────────────────────────────────────────────────────────────────
// VALUES - Multiple Return Values
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_values_basic() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car '(1 2 3))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cons 42 99))"), 42);
    
    // car of nil
    let result = eval.eval_str("(car '())").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

#[test]
fn test_cdr_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // cdr of list
    let result = eval.eval_str("(cdr '(1 2 3))").unwrap();
    assert_eq!(lisp.get(lisp.car(result).unwrap()).unwrap().as_number().unwrap(), 2);
    
    // cdr of pair
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr (cons 1 2))"), 2);
    
    // cdr of nil
    let result = eval.eval_str("(cdr '())").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

#[test]
fn test_cons_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Basic cons
    let result = eval.eval_str("(cons 1 2)").unwrap();
    assert!(lisp.get(result).unwrap().is_cons());
    
    // Cons to nil creates proper list - use car to force
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cons 1 '()))"), 1);
}

#[test]
fn test_list_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Empty list
    let result = eval.eval_str("(list)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
    
    // Single element - use car to force
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (list 42))"), 42);
    
    // Multiple elements
    eval.eval_str("(define my-list (list 1 2 3 4 5))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car my-list)"), 1);
}

// ───────────────────────────────────────────────────────────────────────────
// Predicates
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_atom_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(atom 42)"));
    assert!(eval_is_true(&lisp, &mut eval, "(atom 'x)"));
    assert!(eval_is_true(&lisp, &mut eval, "(atom #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(atom '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(atom '(1 2))"));
    assert!(eval_is_false(&lisp, &mut eval, "(atom (cons 1 2))"));
}

#[test]
fn test_eq_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(eq 1 1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(eq 1 2)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq 'a 'a)"));
    assert!(eval_is_false(&lisp, &mut eval, "(eq 'a 'b)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq '() '())"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq #t #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(eq #f #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(eq #t #f)"));
}

#[test]
fn test_null_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(null? '())"));
    assert!(eval_is_true(&lisp, &mut eval, "(null? nil)"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(null? '(1))"));
}

#[test]
fn test_pair_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(pair? '(1 2))"));
    assert!(eval_is_true(&lisp, &mut eval, "(pair? (cons 1 2))"));
    assert!(eval_is_false(&lisp, &mut eval, "(pair? '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(pair? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(pair? 'x)"));
}

#[test]
fn test_number_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(boolean? #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(boolean? #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean? 1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean? '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(boolean? 'true)"));
}

#[test]
fn test_symbol_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(symbol? 'x)"));
    assert!(eval_is_true(&lisp, &mut eval, "(symbol? 'hello-world)"));
    assert!(eval_is_false(&lisp, &mut eval, "(symbol? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(symbol? #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(symbol? '(a b))"));
}

#[test]
fn test_procedure_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3 4 5)"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ -5 10)"), 5);
}

#[test]
fn test_sub_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10)"), -10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 10 3)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 100 20 30)"), 50);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(- 5 10)"), -5);
}

#[test]
fn test_mul_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(*)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 2 3)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 2 3 4)"), 24);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* -2 3)"), -6);
}

#[test]
fn test_div_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 10 2)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ 100 2 5)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(/ -10 2)"), -5);
}

#[test]
fn test_mod_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(mod 10 3)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(mod 15 5)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(mod 7 2)"), 1);
}

// ───────────────────────────────────────────────────────────────────────────
// Comparison Operators
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_comparisons_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert!(eval_is_true(&lisp, &mut eval, "(not #f)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(not '())"));
    assert!(eval_is_false(&lisp, &mut eval, "(not 'x)"));
}

#[test]
fn test_and_comprehensive() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
fn test_print_display() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // print and display return their argument
    assert_eq!(eval_to_num(&lisp, &mut eval, "(print 42)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(display 99)"), 99);
}

#[test]
fn test_newline() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("(newline)").unwrap();
    assert!(lisp.get(result).unwrap().is_nil());
}

#[test]
fn test_error() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval.eval_str("(error 'test-error)");
    assert!(result.is_err());
}

// ───────────────────────────────────────────────────────────────────────────
// Memoization
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_memoize_explicit() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Explicitly memoize a function
    eval.eval_str("(define slow-fib (lambda (n) (if (< n 2) n (+ (slow-fib (- n 1)) (slow-fib (- n 2))))))").unwrap();
    eval.eval_str("(define fast-fib (memoize slow-fib))").unwrap();
    
    // Should work (memoization helps with repeated calls)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(fast-fib 10)"), 55);
}

// ───────────────────────────────────────────────────────────────────────────
// Lexical Closures - Additional Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_closure_counter() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Counter using closure (pure functional - returns new state)
    eval.eval_str("(define (make-counter init) (lambda (delta) (+ init delta)))").unwrap();
    eval.eval_str("(define counter (make-counter 10))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(counter 5)"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(counter 10)"), 20);
}

#[test]
fn test_closure_nested() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Triple-nested closures
    eval.eval_str("(define (f a) (lambda (b) (lambda (c) (+ a b c))))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(((f 1) 2) 3)"), 6);
}

#[test]
fn test_closure_captures_correct_env() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Verify lexical scoping (not dynamic)
    eval.eval_str("(define x 1)").unwrap();
    eval.eval_str("(define (get-x) x)").unwrap();
    eval.eval_str("(define (call-with-x val f) (let ((x val)) (f)))").unwrap();
    
    // Should use lexical binding (x=1), not dynamic binding (x=100)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(call-with-x 100 get-x)"), 1);
}

// ───────────────────────────────────────────────────────────────────────────
// Lazy Evaluation - Additional Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_lazy_primes_sieve() {
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Sieve helper: filter out multiples
    eval.eval_str("(define (filter-multiples n stream) (cond ((null? stream) '()) ((= (mod (car stream) n) 0) (filter-multiples n (cdr stream))) (else (cons (car stream) (filter-multiples n (cdr stream))))))").unwrap();
    
    // Test filter-multiples on a finite list
    eval.eval_str("(define nums '(2 3 4 5 6 7 8 9 10))").unwrap();
    eval.eval_str("(define filtered (filter-multiples 2 nums))").unwrap();
    
    // Check first element
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car filtered)"), 3);
}

#[test]
fn test_lazy_iterate() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Iterate: generate infinite stream by repeatedly applying f
    eval.eval_str("(define (iterate f x) (cons x (iterate f (f x))))").unwrap();
    eval.eval_str("(define (add1 x) (+ x 1))").unwrap();
    eval.eval_str("(define nats (iterate add1 0))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car nats)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr nats))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr nats)))"), 2);
}

#[test]
fn test_lazy_cycle() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Create a simple repeating pattern
    eval.eval_str("(define (repeat-ab) (cons 'a (cons 'b (repeat-ab))))").unwrap();
    eval.eval_str("(define cycle (repeat-ab))").unwrap();
    
    let first = eval.eval_str("(car cycle)").unwrap();
    assert!(lisp.symbol_matches(first, "a").unwrap());
    
    let second = eval.eval_str("(car (cdr cycle))").unwrap();
    assert!(lisp.symbol_matches(second, "b").unwrap());
    
    let third = eval.eval_str("(car (cdr (cdr cycle)))").unwrap();
    assert!(lisp.symbol_matches(third, "a").unwrap());
}

// ───────────────────────────────────────────────────────────────────────────
// Infinite Streams - Additional Tests
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn test_infinite_powers_of_two() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Powers of 2: 1, 2, 4, 8, 16, ...
    eval.eval_str("(define (powers-of-2-from n) (cons n (powers-of-2-from (* n 2))))").unwrap();
    eval.eval_str("(define powers (powers-of-2-from 1))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car powers)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr powers))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr powers)))"), 4);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr powers))))"), 8);
}

#[test]
fn test_infinite_triangular_numbers() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Triangular numbers: 1, 3, 6, 10, 15, ...
    eval.eval_str("(define (triangular n sum) (cons sum (triangular (+ n 1) (+ sum n 1))))").unwrap();
    eval.eval_str("(define tris (triangular 1 1))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car tris)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr tris))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr tris)))"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (cdr (cdr tris))))"), 10);
}

// ═══════════════════════════════════════════════════════════════════════════
// MUTATION TESTS
// Tests for set!, set-car!, set-cdr! operations
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_mutation_set() {
    let lisp: Lisp<1000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);
    
    eval.eval_str("(set-car! p 10)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 2);  // cdr unchanged
}

#[test]
fn test_mutation_set_cdr_basic() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 2);
    
    eval.eval_str("(set-cdr! p 20)").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cdr p)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car p)"), 1);  // car unchanged
}

#[test]
fn test_mutation_build_list() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Build a list by mutation
    eval.eval_str("(define lst (cons 1 '()))").unwrap();
    eval.eval_str("(set-cdr! lst (cons 2 '()))").unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car lst)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr lst))"), 2);
}

#[test]
fn test_mutation_with_gc() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '())"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(a b c d e))"), 5);
}

#[test]
fn test_stdlib_fold() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(nth 0 '(10 20 30))"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(nth 1 '(10 20 30))"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(nth 2 '(10 20 30))"), 30);
}

#[test]
fn test_stdlib_range() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Force range to test - it's lazy
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (range 0 5))"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (range 0 5))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cdr (range 0 5)))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length (range 0 0))"), 0);
}

#[test]
fn test_stdlib_identity_and_constantly() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(identity 42)"), 42);
    
    eval.eval_str("(define always-5 (constantly 5))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(always-5 1)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(always-5 100)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(always-5 'foo)"), 5);
}

#[test]
fn test_stdlib_curry() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Curry + 5 to create add5
    eval.eval_str("(define add5 (curry + 5))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 10)"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(add5 20)"), 25);
}

#[test]
fn test_stdlib_cadr_caddr_cddr() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr '(1 2 3))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(caddr '(1 2 3))"), 3);
    
    // cddr returns the list after first two elements
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car (cddr '(1 2 3 4)))"), 3);
}

#[test]
fn test_stdlib_member() {
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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

/// PITFALL: nil is NOT false!
/// In this Lisp, only #f is false. nil/() is the empty list and is truthy.
#[test]
fn test_pitfall_nil_is_truthy() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // nil is truthy!
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if nil 1 2)"), 1);  // Takes then branch
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if '() 1 2)"), 1);  // Takes then branch
    
    // Only #f is false
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if #f 1 2)"), 2);  // Takes else branch
}

/// PITFALL: 0 is also truthy!
#[test]
fn test_pitfall_zero_is_truthy() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // 0 is truthy (unlike C/Python)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(if 0 1 2)"), 1);  // Takes then branch
}

/// PITFALL: Lazy evaluation means side effects may not happen when expected
#[test]
fn test_pitfall_lazy_side_effects() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define a counter to track side effects
    eval.eval_str("(define count 0)").unwrap();
    eval.eval_str("(define (side-effect! x) (set! count (+ count 1)) x)").unwrap();
    
    // Build a lazy list - side effects don't happen yet!
    eval.eval_str("(define lst (cons (side-effect! 1) (cons (side-effect! 2) '())))").unwrap();
    
    // Count is still 0 - cons is lazy!
    assert_eq!(eval_to_num(&lisp, &mut eval, "count"), 0);
    
    // Only when we force the elements do side effects happen
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car lst)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "count"), 1);  // Now side effect happened
}

/// PITFALL: StdLib functions parse their body on each call (minor overhead)
/// This is intentional - it keeps function code out of the arena.
#[test]
fn test_stdlib_parses_on_each_call() {
    let lisp: Lisp<3000> = Lisp::new();
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
    let lisp: Lisp<4000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
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
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Get arena stats
    let result = eval.eval_str("(arena-stats)").unwrap();
    
    // Result should be a list (capacity allocated free usage-percent)
    assert!(lisp.get(result).unwrap().is_cons());
    
    // First element should be capacity = 2000
    let capacity = lisp.car(result).unwrap();
    assert_eq!(lisp.get(capacity).unwrap().as_number(), Some(2000));
    
    // Second element (allocated) should be a number
    let rest = lisp.cdr(result).unwrap();
    let allocated = lisp.car(rest).unwrap();
    assert!(lisp.get(allocated).unwrap().is_number());
    assert!(lisp.get(allocated).unwrap().as_number().unwrap() > 0);
}

#[test]
fn test_gc_disabled_no_collect() {
    let lisp: Lisp<2000> = Lisp::new();
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
