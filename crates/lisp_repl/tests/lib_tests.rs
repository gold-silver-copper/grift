use lisp_repl::*;

#[test]
fn test_format_number() {
    let lisp: Lisp<100> = Lisp::new();
    let idx = lisp.number(42).unwrap();
    assert_eq!(value_to_string(&lisp, idx), "42");
}

#[test]
fn test_format_booleans() {
    let lisp: Lisp<100> = Lisp::new();
    
    let t = lisp.true_val().unwrap();
    assert_eq!(value_to_string(&lisp, t), "#t");
    
    let f = lisp.false_val().unwrap();
    assert_eq!(value_to_string(&lisp, f), "#f");
}

#[test]
fn test_format_nil() {
    let lisp: Lisp<100> = Lisp::new();
    let idx = lisp.nil().unwrap();
    assert_eq!(value_to_string(&lisp, idx), "()");
}

#[test]
fn test_format_symbol() {
    let lisp: Lisp<100> = Lisp::new();
    let idx = lisp.symbol("hello").unwrap();
    assert_eq!(value_to_string(&lisp, idx), "hello");
}

#[test]
fn test_format_list() {
    let lisp: Lisp<100> = Lisp::new();
    let a = lisp.number(1).unwrap();
    let b = lisp.number(2).unwrap();
    let c = lisp.number(3).unwrap();
    let nil = lisp.nil().unwrap();
    let list = lisp.cons(c, nil).unwrap();
    let list = lisp.cons(b, list).unwrap();
    let list = lisp.cons(a, list).unwrap();
    assert_eq!(value_to_string(&lisp, list), "(1 2 3)");
}

#[test]
fn test_format_dotted_pair() {
    let lisp: Lisp<100> = Lisp::new();
    let a = lisp.number(1).unwrap();
    let b = lisp.number(2).unwrap();
    let pair = lisp.cons(a, b).unwrap();
    assert_eq!(value_to_string(&lisp, pair), "(1 . 2)");
}

#[test]
fn test_format_lambda() {
    let lisp: Lisp<100> = Lisp::new();
    let params = lisp.nil().unwrap();
    let body = lisp.number(42).unwrap();
    let env = lisp.nil().unwrap();
    let lambda = lisp.lambda(params, body, env).unwrap();
    assert_eq!(value_to_string(&lisp, lambda), "#<lambda>");
}

#[test]
fn test_eval_and_format() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    assert_eq!(eval_to_string(&lisp, &mut eval, "(+ 1 2)").unwrap(), "3");
    assert_eq!(eval_to_string(&lisp, &mut eval, "'(a b c)").unwrap(), "(a b c)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "#t").unwrap(), "#t");
    assert_eq!(eval_to_string(&lisp, &mut eval, "#f").unwrap(), "#f");
}

#[test]
fn test_factorial() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (fact n) (if (= n 0) 1 (* n (fact (- n 1)))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(fact 10)").unwrap(), "3628800");
}

#[test]
fn test_tco_recursion() {
    // Hybrid evaluation: tail calls are STRICT, so TCO works properly!
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (sum-to n acc) (if (= n 0) acc (sum-to (- n 1) (+ acc n))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(sum-to 100 0)").unwrap(), "5050");
}

#[test]
fn test_fibonacci() {
    let lisp: Lisp<5000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(fib 10)").unwrap(), "55");
}

#[test]
fn test_higher_order() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (twice f x) (f (f x)))").unwrap();
    eval.eval_str("(define (add1 x) (+ x 1))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(twice add1 5)").unwrap(), "7");
}

#[test]
fn test_closures() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (make-adder n) (lambda (x) (+ x n)))").unwrap();
    eval.eval_str("(define add5 (make-adder 5))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(add5 10)").unwrap(), "15");
}

#[test]
fn test_list_operations() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define length
    eval.eval_str("(define (length lst) (if (null? lst) 0 (+ 1 (length (cdr lst)))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(length '(1 2 3))").unwrap(), "3");
    
    // Define append
    eval.eval_str("(define (append a b) (if (null? a) b (cons (car a) (append (cdr a) b))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(append '(1 2) '(3))").unwrap(), "(1 2 3)");
}

#[test]
fn test_map() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (map f lst) (if (null? lst) '() (cons (f (car lst)) (map f (cdr lst)))))").unwrap();
    eval.eval_str("(define (square x) (* x x))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(map square '(1 2 3))").unwrap(), "(1 4 9)");
}

#[test]
fn test_filter() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (filter pred lst) (cond ((null? lst) '()) ((pred (car lst)) (cons (car lst) (filter pred (cdr lst)))) (else (filter pred (cdr lst)))))").unwrap();
    // Using modulo instead of mod
    eval.eval_str("(define (even x) (= (modulo x 2) 0))").unwrap();
    // Small list
    assert_eq!(eval_to_string(&lisp, &mut eval, "(filter even '(1 2 3 4))").unwrap(), "(2 4)");
}

#[test]
fn test_fold() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (fold f acc lst) (if (null? lst) acc (fold f (f acc (car lst)) (cdr lst))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(fold + 0 '(1 2 3 4 5))").unwrap(), "15");
}

// ═══════════════════════════════════════════════════════════════════════════
// STRICT EVALUATION TESTS
// All arguments are evaluated before function application (call-by-value)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_strict_basic() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple computations work
    assert_eq!(eval_to_string(&lisp, &mut eval, "(+ 1 2 3)").unwrap(), "6");
    
    eval.eval_str("(define (add x y) (+ x y))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(add 10 20)").unwrap(), "30");
}

#[test]
fn test_strict_cons() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(car p)").unwrap(), "1");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(cdr p)").unwrap(), "2");
}

#[test]
fn test_strict_if_branches() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Only selected branch is evaluated (if is still a special form)
    eval.eval_str("(define (safe-div x y) (if (= y 0) 0 (/ x y)))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(safe-div 10 0)").unwrap(), "0");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(safe-div 10 2)").unwrap(), "5");
}

#[test]
fn test_strict_evaluation() {
    // All function arguments are evaluated before application
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Function args are strict
    eval.eval_str("(define (first x y) x)").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(first 42 100)").unwrap(), "42");
    
    // Special forms like 'if' still only evaluate the selected branch
    assert_eq!(eval_to_string(&lisp, &mut eval, "(if #t 'yes 'no)").unwrap(), "yes");
}

#[test]
fn test_empty_list_is_truthy() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // The empty list '() is truthy (in Scheme, only #f is false)
    // Note: 'nil' is just a regular symbol, not the empty list
    assert_eq!(eval_to_string(&lisp, &mut eval, "(if '() 'yes 'no)").unwrap(), "yes");
    
    // Only #f is false
    assert_eq!(eval_to_string(&lisp, &mut eval, "(if #f 'yes 'no)").unwrap(), "no");
}
