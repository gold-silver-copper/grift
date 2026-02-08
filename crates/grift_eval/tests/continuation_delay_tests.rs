mod common;

use grift_eval::*;
use common::{eval_to_string, eval_to_num, eval_is_false};

// ============================================================
// Section 5.6: Continuations
// ============================================================

/// member using call/cc for nonlocal exit from a do loop
#[test]
fn test_member_callcc_not_found() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define member-cc
          (lambda (x ls)
            (call/cc
              (lambda (break)
                (do ((ls ls (cdr ls)))
                    ((null? ls) #f)
                    (if (equal? x (car ls))
                        (break ls)))))))",
    )
    .unwrap();

    // (member-cc 'd '(a b c)) => #f
    assert!(eval_is_false(&lisp, &mut eval, "(member-cc 'd '(a b c))"));
}

#[test]
fn test_member_callcc_found() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define member-cc
          (lambda (x ls)
            (call/cc
              (lambda (break)
                (do ((ls ls (cdr ls)))
                    ((null? ls) #f)
                    (if (equal? x (car ls))
                        (break ls)))))))",
    )
    .unwrap();

    // (member-cc 'b '(a b c)) => (b c)
    assert_eq!(
        eval_to_string(&lisp, &mut eval, "(member-cc 'b '(a b c))"),
        "(b c)"
    );
}

/// dynamic-wind basic: in/body/out thunks called in order
#[test]
fn test_dynamic_wind_basic() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define log '())").unwrap();

    let result = eval
        .eval_str(
            "(dynamic-wind
              (lambda () (set! log (cons 'in log)))
              (lambda () (set! log (cons 'body log)) 42)
              (lambda () (set! log (cons 'out log))))",
        )
        .unwrap();

    // Body returns 42
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));

    // log should be (out body in) — most recent first
    assert_eq!(eval_to_string(&lisp, &mut eval, "log"), "(out body in)");
}

/// unwind-protect macro: cleanup runs even on continuation escape
#[test]
fn test_unwind_protect() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define-syntax unwind-protect
          (syntax-rules ()
            ((_ body cleanup ...)
             (dynamic-wind
               (lambda () #f)
               (lambda () body)
               (lambda () cleanup ...)))))",
    )
    .unwrap();

    // ((call/cc
    //    (let ((x 'a))
    //      (lambda (k)
    //        (unwind-protect
    //          (k (lambda () x))
    //          (set! x 'b)))))) => b
    let result = eval
        .eval_str(
            "((call/cc
               (let ((x 'a))
                 (lambda (k)
                   (unwind-protect
                     (k (lambda () x))
                     (set! x 'b))))))",
        )
        .unwrap();
    assert!(lisp.symbol_matches(result, "b").unwrap());
}

/// fluid-let basic: temporary binding restored after exit
#[test]
fn test_fluid_let_basic() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define-syntax fluid-let
          (syntax-rules ()
            ((_ ((x v)) e1 e2 ...)
             (let ((y v))
               (let ((swap (lambda ()
                             (let ((t x))
                               (set! x y)
                               (set! y t)))))
                 (dynamic-wind
                   swap
                   (lambda () e1 e2 ...)
                   swap))))))",
    )
    .unwrap();

    // (let ((x 3))
    //   (+ (fluid-let ((x 5)) x) x)) => 8
    assert_eq!(
        eval_to_num(
            &lisp,
            &mut eval,
            "(let ((x 3))
              (+ (fluid-let ((x 5))
                   x)
                 x))"
        ),
        8
    );
}

/// fluid-let with call/cc: variable reverts on continuation escape
#[test]
fn test_fluid_let_callcc_revert() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define-syntax fluid-let
          (syntax-rules ()
            ((_ ((x v)) e1 e2 ...)
             (let ((y v))
               (let ((swap (lambda ()
                             (let ((t x))
                               (set! x y)
                               (set! y t)))))
                 (dynamic-wind
                   swap
                   (lambda () e1 e2 ...)
                   swap))))))",
    )
    .unwrap();

    // (let ((x 'a))
    //   (let ((f (lambda () x)))
    //     (cons (call/cc
    //             (lambda (k)
    //               (fluid-let ((x 'b))
    //                 (f))))
    //           (f)))) => (b . a)
    assert_eq!(
        eval_to_string(
            &lisp,
            &mut eval,
            "(let ((x 'a))
              (let ((f (lambda () x)))
                (cons (call/cc
                        (lambda (k)
                          (fluid-let ((x 'b))
                            (f))))
                      (f))))"
        ),
        "(b . a)"
    );
}

/// fluid-let with reenter: continuation re-invocation reinstates temporary value
#[test]
fn test_fluid_let_reenter() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define-syntax fluid-let
          (syntax-rules ()
            ((_ ((x v)) e1 e2 ...)
             (let ((y v))
               (let ((swap (lambda ()
                             (let ((t x))
                               (set! x y)
                               (set! y t)))))
                 (dynamic-wind
                   swap
                   (lambda () e1 e2 ...)
                   swap))))))",
    )
    .unwrap();

    eval.eval_str("(define reenter #f)").unwrap();
    eval.eval_str("(define x 0)").unwrap();

    // First invocation returns 2
    assert_eq!(
        eval_to_num(
            &lisp,
            &mut eval,
            "(fluid-let ((x 1))
              (call/cc (lambda (k) (set! reenter k)))
              (set! x (+ x 1))
              x)"
        ),
        2
    );

    // x should be 0 after fluid-let exits
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 0);

    // Re-entering: x was 2 inside, so now becomes 3
    assert_eq!(eval_to_num(&lisp, &mut eval, "(reenter '*)"), 3);

    // Re-entering again: x was 3 inside, so now becomes 4
    assert_eq!(eval_to_num(&lisp, &mut eval, "(reenter '*)"), 4);

    // x is still 0 outside
    assert_eq!(eval_to_num(&lisp, &mut eval, "x"), 0);
}

// ============================================================
// Section 5.7: Delayed Evaluation
// ============================================================

/// Basic delay and force
#[test]
fn test_delay_force_basic() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(
        eval_to_num(&lisp, &mut eval, "(force (delay (+ 1 2)))"),
        3
    );
}

/// delay memoizes: expression evaluated only once
#[test]
fn test_delay_memoization() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(define count 0)").unwrap();
    eval.eval_str(
        "(define p (delay (begin (set! count (+ count 1)) count)))",
    )
    .unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force p)"), 1);
    // count should be 1 — only evaluated once
    assert_eq!(eval_to_num(&lisp, &mut eval, "count"), 1);
}

/// Stream abstraction: stream-car, stream-cdr, counters
#[test]
fn test_streams_basic() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define stream-car
          (lambda (s)
            (car (force s))))",
    )
    .unwrap();

    eval.eval_str(
        "(define stream-cdr
          (lambda (s)
            (cdr (force s))))",
    )
    .unwrap();

    eval.eval_str(
        "(define counters
          (let next ((n 1))
            (delay (cons n (next (+ n 1))))))",
    )
    .unwrap();

    // (stream-car counters) => 1
    assert_eq!(eval_to_num(&lisp, &mut eval, "(stream-car counters)"), 1);

    // (stream-car (stream-cdr counters)) => 2
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "(stream-car (stream-cdr counters))"),
        2
    );
}

/// stream-add and even-counters
#[test]
fn test_stream_add() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str(
        "(define stream-car
          (lambda (s)
            (car (force s))))",
    )
    .unwrap();

    eval.eval_str(
        "(define stream-cdr
          (lambda (s)
            (cdr (force s))))",
    )
    .unwrap();

    eval.eval_str(
        "(define counters
          (let next ((n 1))
            (delay (cons n (next (+ n 1))))))",
    )
    .unwrap();

    eval.eval_str(
        "(define stream-add
          (lambda (s1 s2)
            (delay (cons
                     (+ (stream-car s1) (stream-car s2))
                     (stream-add (stream-cdr s1) (stream-cdr s2))))))",
    )
    .unwrap();

    eval.eval_str("(define even-counters (stream-add counters counters))")
        .unwrap();

    // (stream-car even-counters) => 2
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "(stream-car even-counters)"),
        2
    );

    // (stream-car (stream-cdr even-counters)) => 4
    assert_eq!(
        eval_to_num(
            &lisp,
            &mut eval,
            "(stream-car (stream-cdr even-counters))"
        ),
        4
    );
}

/// make-promise and force definitions from the spec
#[test]
fn test_make_promise_force() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Use the spec's definitions
    eval.eval_str(
        "(define make-promise
          (lambda (p)
            (let ((val #f) (set? #f))
              (lambda ()
                (if (not set?)
                    (let ((x (p)))
                      (if (not set?)
                          (begin (set! val x)
                                 (set! set? #t)))))
                val))))",
    )
    .unwrap();

    eval.eval_str(
        "(define my-force
          (lambda (promise)
            (promise)))",
    )
    .unwrap();

    eval.eval_str(
        "(define my-delay-val (make-promise (lambda () (+ 10 20))))",
    )
    .unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(my-force my-delay-val)"), 30);
    // Second force returns memoized value
    assert_eq!(eval_to_num(&lisp, &mut eval, "(my-force my-delay-val)"), 30);
}
