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
fn test_format_thunk() {
    let lisp: Lisp<100> = Lisp::new();
    let expr = lisp.number(42).unwrap();
    let env = lisp.nil().unwrap();
    let thunk = lisp.thunk(expr, env).unwrap();
    assert_eq!(value_to_string(&lisp, thunk), "#<promise>");
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
    // NOTE: Lazy evaluation limits recursion depth
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define length - works on small lists
    eval.eval_str("(define (length lst) (if (null? lst) 0 (+ 1 (length (cdr lst)))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(length '(1 2 3))").unwrap(), "3");
    
    // Define append - small lists
    eval.eval_str("(define (append a b) (if (null? a) b (cons (car a) (append (cdr a) b))))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(append '(1 2) '(3))").unwrap(), "(1 2 3)");
}

#[test]
fn test_map() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (map f lst) (if (null? lst) '() (cons (f (car lst)) (map f (cdr lst)))))").unwrap();
    eval.eval_str("(define (square x) (* x x))").unwrap();
    // Small list to avoid thunk accumulation
    assert_eq!(eval_to_string(&lisp, &mut eval, "(map square '(1 2 3))").unwrap(), "(1 4 9)");
}

#[test]
fn test_filter() {
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define (filter pred lst) (cond ((null? lst) '()) ((pred (car lst)) (cons (car lst) (filter pred (cdr lst)))) (else (filter pred (cdr lst)))))").unwrap();
    eval.eval_str("(define (even x) (= (mod x 2) 0))").unwrap();
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
// LAZY EVALUATION TESTS
// Everything is lazy by default - no delay/force needed!
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lazy_basic() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Simple computations work
    assert_eq!(eval_to_string(&lisp, &mut eval, "(+ 1 2 3)").unwrap(), "6");
    
    eval.eval_str("(define (add x y) (+ x y))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(add 10 20)").unwrap(), "30");
}

#[test]
fn test_lazy_cons() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str("(define p (cons 1 2))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(car p)").unwrap(), "1");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(cdr p)").unwrap(), "2");
}

#[test]
fn test_lazy_infinite_stream() {
    // Infinite structures work automatically in lazy language
    let lisp: Lisp<3000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Generator for infinite stream of 1s
    eval.eval_str("(define (make-ones) (cons 1 (make-ones)))").unwrap();
    eval.eval_str("(define ones (make-ones))").unwrap();
    
    // Can access elements without infinite loop
    assert_eq!(eval_to_string(&lisp, &mut eval, "(car ones)").unwrap(), "1");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(car (cdr ones))").unwrap(), "1");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(car (cdr (cdr ones)))").unwrap(), "1");
}

#[test]
fn test_lazy_if_branches() {
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Only selected branch is evaluated
    eval.eval_str("(define (safe-div x y) (if (= y 0) 0 (/ x y)))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(safe-div 10 0)").unwrap(), "0");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(safe-div 10 2)").unwrap(), "5");
}

#[test]
fn test_hybrid_evaluation() {
    // HYBRID: Lambda args are strict, but builtin args are lazy
    let lisp: Lisp<2000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Lambda args are strict (for TCO)
    eval.eval_str("(define (first x y) x)").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(first 42 100)").unwrap(), "42");
    
    // But special forms like 'if' have lazy branches
    assert_eq!(eval_to_string(&lisp, &mut eval, "(if #t 'yes undefined)").unwrap(), "yes");
    
    // cons is non-strict - enables infinite streams
    eval.eval_str("(define (ones) (cons 1 (ones)))").unwrap();
    assert_eq!(eval_to_string(&lisp, &mut eval, "(car (ones))").unwrap(), "1");
}

#[test]
fn test_nil_is_truthy() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // nil is truthy
    assert_eq!(eval_to_string(&lisp, &mut eval, "(if nil 'yes 'no)").unwrap(), "yes");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(if '() 'yes 'no)").unwrap(), "yes");
    
    // Only #f is false
    assert_eq!(eval_to_string(&lisp, &mut eval, "(if #f 'yes 'no)").unwrap(), "no");
}
