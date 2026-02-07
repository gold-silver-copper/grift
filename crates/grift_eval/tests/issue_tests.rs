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

// Issue 3: identifier-syntax - macros that expand in bare identifier position
#[test]
fn test_identifier_syntax_basic() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // identifier-syntax is now in the standard library (macros.scm)
    // Simple test: identifier macro that evaluates to a constant
    eval.eval_str(r#"
        (define-syntax my-val
          (identifier-syntax 42))
    "#).unwrap();
    
    let result = eval_to_num(&lisp, &mut eval, "my-val");
    assert_eq!(result, 42);
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

// Issue 5: cond with lexically bound else should NOT match the else literal
// Per R7RS, syntax-rules literals are matched via free-identifier=?.
// When `else` is locally bound (e.g., by let), it's a different identifier
// than the `else` in the cond macro's literal list.
#[test]
fn test_cond_bound_else_not_matched() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // (let ((else #f)) (cond (else 42))) should NOT match the else clause
    // because else is locally bound. Instead, else is used as a test expression
    // and evaluates to #f, so the cond returns #f.
    let result = eval_to_string(&lisp, &mut eval, r#"
        (let ((else #f))
          (cond (else 42)))
    "#);
    assert_eq!(result, "#f");
}

// Issue 6: identifier-syntax with set! and variable mutation
// The full R6RS identifier-syntax example with x++
#[test]
fn test_identifier_syntax_with_set() {
    let lisp: Lisp<80000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // identifier-syntax is now in the standard library (macros.scm)
    let result = eval_to_string(&lisp, &mut eval, r#"
        (let ((x 0))
          (define-syntax x++
            (identifier-syntax
              (let ((t x)) (set! x (+ t 1)) t)))
          (let ((a x++))
            (list a x)))
    "#);
    assert_eq!(result, "(0 1)");
}

// Issue 7: Redefined cond with explicit free-identifier=? in fender
// When cond is redefined using syntax-case with free-identifier=? to check
// for `else`, a locally bound `else` should NOT match the macro's `else`.
// This stress tests free-identifier=? with bare symbols from pattern variables
// vs. template identifiers.
#[test]
fn test_redefined_cond_free_identifier_eq() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Redefine cond using syntax-case with explicit free-identifier=?
    eval.eval_str(r#"
        (define-syntax my-cond
          (lambda (x)
            (syntax-case x ()
              ((_ (e0 e1 e2 ...))
               (and (identifier? (syntax e0))
                    (free-identifier=? (syntax e0) (syntax else)))
               (syntax (begin e1 e2 ...)))
              ((_ (e0 e1 e2 ...)) (syntax (if e0 (begin e1 e2 ...))))
              ((_ (e0 e1 e2 ...) c1 c2 ...)
               (syntax (if e0 (begin e1 e2 ...) (my-cond c1 c2 ...)))))))
    "#).unwrap();

    // Without local binding: else is the keyword, first clause should match
    let result = eval_to_string(&lisp, &mut eval, "(my-cond (else 42))");
    assert_eq!(result, "42");

    // With local binding: else is #f, first clause should NOT match
    // The second clause matches: (if else (begin 42)) => (if #f (begin 42)) => #f
    let result = eval_to_string(&lisp, &mut eval, r#"
        (let ((else #f))
          (my-cond (else 42)))
    "#);
    assert_eq!(result, "#f",
        "locally bound else should NOT match the else keyword in free-identifier=?");
}
