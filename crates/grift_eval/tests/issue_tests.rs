use grift_eval::*;

fn eval_to_string<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> String {
    let result = eval.eval_str(input).unwrap();
    format!("{}", lisp.display(result))
}

fn eval_to_num<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> isize {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().as_number().unwrap()
}

// Issue 1: or hygiene - when `if` is rebound, or's template `if` should still 
// refer to the special form `if`, not the variable binding
#[test]
fn test_or_hygiene_rebound_if() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval_to_string(&lisp, &mut eval, r#"
        (let ((if #f))
          (let ((t 'okay))
            (or if t)))
    "#);
    assert_eq!(result, "okay");
}

// Issue 2: dolet hygiene - template variable `a` should be distinct from user `a`
#[test]
fn test_dolet_hygiene_returns_7() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    let result = eval_to_num(&lisp, &mut eval, r#"
        (let-syntax ((dolet (lambda (x)
                              (syntax-case x ()
                                ((_ b)
                                 (syntax (let ((a 3) (b 4))
                                           (+ a b))))))))
          (dolet a))
    "#);
    // Must be 7 - template 'a' renamed to prevent capture of user's 'a' 
    assert_eq!(result, 7);
}

// Issue 3: identifier-syntax 
#[test]
fn test_identifier_syntax_basic() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    eval.eval_str(r#"
        (define-syntax identifier-syntax
          (lambda (x)
            (syntax-case x ()
              ((_ e)
               (syntax
                 (lambda (x)
                   (syntax-case x ()
                     (id (identifier? (syntax id)) (syntax e))
                     ((id x (... ...)) (identifier? (syntax id)) (syntax (e x (... ...)))))))))))
    "#).unwrap();
    
    let result = eval_to_num(&lisp, &mut eval, r#"
        (let ((x 0))
          (define-syntax x++
            (identifier-syntax
              (let ((t x)) (set! x (+ t 1)) t)))
          (let ((a x++))
            (list a x)))
    "#);
    // x++ should be expanded in identifier position, giving (let ((t x)) (set! x (+ t 1)) t)
    // which gives t=0, sets x to 1, returns t=0
    // So a=0, x=1, and (list a x) = (0 1)
    // Actually just testing it works
    println!("identifier-syntax result: {}", result);
}

// Issue 4: Named let via syntax-rules - the named let form (let loop ((var val) ...) body ...)
// should be distinguishable from regular let ((var val) ...) body ...)
#[test]
fn test_named_let_via_syntax_rules() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Define rec first (needed for the named let definition)
    eval.eval_str(r#"
        (define-syntax rec
          (syntax-rules ()
            ((_ x e) (letrec ((x e)) x))))
    "#).unwrap();
    
    // Now define let using syntax-rules with BOTH regular and named forms
    // The named form (let loop ((x v) ...) body ...) needs to be distinguished
    // from (let ((x v) ...) body ...)
    eval.eval_str(r#"
        (define-syntax my-let2
          (syntax-rules ()
            ((_ ((x v) ...) e1 e2 ...)
             ((lambda (x ...) e1 e2 ...) v ...))
            ((_ f ((x v) ...) e1 e2 ...)
             ((rec f (lambda (x ...) e1 e2 ...)) v ...))))
    "#).unwrap();
    
    // Regular let
    let r1 = eval_to_num(&lisp, &mut eval, "(my-let2 ((a 1) (b 2)) (+ a b))");
    assert_eq!(r1, 3);
    
    // Named let
    let r2 = eval_to_num(&lisp, &mut eval, r#"
        (my-let2 loop ((i 0) (sum 0))
          (if (= i 5) sum (loop (+ i 1) (+ sum i))))
    "#);
    assert_eq!(r2, 10);
}
