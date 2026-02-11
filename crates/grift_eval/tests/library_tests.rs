mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_is_false};

// ============================================================================
// define-library & import basics
// ============================================================================

#[test]
fn test_define_library_and_import() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("
        (define-library (test lib)
          (export greet)
          (begin
            (define (greet) 42)))
    ").unwrap();

    eval.eval_str("(import (test lib))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(greet)"), 42);
}

#[test]
fn test_library_isolation() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Define a library with an internal helper not exported
    eval.eval_str("
        (define-library (test isolation)
          (export public-fn)
          (begin
            (define (helper x) (* x 2))
            (define (public-fn x) (helper x))))
    ").unwrap();

    eval.eval_str("(import (test isolation))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(public-fn 5)"), 10);
}

#[test]
fn test_library_no_exports_exports_all() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Library with no export declaration exports everything
    eval.eval_str("
        (define-library (test all)
          (begin
            (define (foo) 1)
            (define (bar) 2)))
    ").unwrap();

    eval.eval_str("(import (test all))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(foo)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bar)"), 2);
}

// ============================================================================
// Auto-loading from embedded library sources
// ============================================================================

#[test]
fn test_import_scheme_base() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // (scheme base) should auto-load from embedded sources
    eval.eval_str("(import (scheme base))").unwrap();
    // car, +, etc. should be available (they already are from globals,
    // but the import should succeed without error)
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2)"), 3);
}

#[test]
fn test_import_scheme_cxr() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme cxr))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr '(1 2 3))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(caddr '(1 2 3))"), 3);
}

#[test]
fn test_import_scheme_char() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme char))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(char-alphabetic? #\\a)"));
    assert!(eval_is_false(&lisp, &mut eval, "(char-alphabetic? #\\1)"));
}

#[test]
fn test_import_scheme_write() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Should succeed without error
    eval.eval_str("(import (scheme write))").unwrap();
}

#[test]
fn test_import_scheme_lazy() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme lazy))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(force (delay 42))"), 42);
}

#[test]
fn test_import_scheme_inexact() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme inexact))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(finite? 1)"));
}

#[test]
fn test_import_scheme_file() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // import should succeed; actual file ops require an I/O provider
    eval.eval_str("(import (scheme file))").unwrap();
}

#[test]
fn test_import_scheme_eval() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme eval))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(eval '(+ 1 2))"), 3);
}

#[test]
fn test_import_scheme_process_context() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Should succeed without error
    eval.eval_str("(import (scheme process-context))").unwrap();
}

#[test]
fn test_import_scheme_read() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme read))").unwrap();
}

#[test]
fn test_import_scheme_repl() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme repl))").unwrap();
}

#[test]
fn test_import_scheme_time() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme time))").unwrap();
}

#[test]
fn test_import_scheme_case_lambda() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme case-lambda))").unwrap();
    // case-lambda should be available as a macro
    assert_eq!(eval_to_num(&lisp, &mut eval, "
        (let ((f (case-lambda
                   (() 0)
                   ((x) x)
                   ((x y) (+ x y)))))
          (f 3 4))
    "), 7);
}

// ============================================================================
// Import modifiers
// ============================================================================

#[test]
fn test_import_only() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("
        (define-library (test mod)
          (export a b c)
          (begin
            (define a 1)
            (define b 2)
            (define c 3)))
    ").unwrap();

    eval.eval_str("(import (only (test mod) a c))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "a"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "c"), 3);
}

#[test]
fn test_import_except() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("
        (define-library (test except-mod)
          (export aa bb cc)
          (begin
            (define aa 10)
            (define bb 20)
            (define cc 30)))
    ").unwrap();

    eval.eval_str("(import (except (test except-mod) bb))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "aa"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "cc"), 30);
}

#[test]
fn test_import_prefix() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("
        (define-library (test prefix-mod)
          (export val)
          (begin
            (define val 99)))
    ").unwrap();

    eval.eval_str("(import (prefix (test prefix-mod) my-))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "my-val"), 99);
}

#[test]
fn test_import_rename() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("
        (define-library (test rename-mod)
          (export original)
          (begin
            (define original 77)))
    ").unwrap();

    eval.eval_str("(import (rename (test rename-mod) (original renamed)))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "renamed"), 77);
}

// ============================================================================
// Macro export/import through the library system
// ============================================================================

#[test]
fn test_library_exports_macros() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("
        (define-library (test macros)
          (export my-if)
          (begin
            (define-syntax my-if
              (syntax-rules ()
                ((my-if test then else)
                 (cond (test then) (#t else)))))))
    ").unwrap();

    eval.eval_str("(import (test macros))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(my-if #t 1 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(my-if #f 1 2)"), 2);
}

#[test]
fn test_library_exports_both_macros_and_procedures() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("
        (define-library (test mixed)
          (export my-when double)
          (begin
            (define (double x) (* x 2))
            (define-syntax my-when
              (syntax-rules ()
                ((my-when test body ...)
                 (if test (begin body ...) (if #f #f)))))))
    ").unwrap();

    eval.eval_str("(import (test mixed))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(double 5)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(my-when #t 42)"), 42);
}

// ============================================================================
// Lazy loading / multiple imports
// ============================================================================

#[test]
fn test_multiple_imports() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme cxr) (scheme char))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr '(1 2 3))"), 2);
    assert!(eval_is_true(&lisp, &mut eval, "(char-alphabetic? #\\z)"));
}

#[test]
fn test_library_with_dependency() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // First library
    eval.eval_str("
        (define-library (test dep-base)
          (export base-val)
          (begin (define base-val 100)))
    ").unwrap();

    // Second library depends on first
    eval.eval_str("
        (define-library (test dep-user)
          (export derived-val)
          (import (test dep-base))
          (begin (define derived-val (+ base-val 1))))
    ").unwrap();

    eval.eval_str("(import (test dep-user))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "derived-val"), 101);
}

// ============================================================================
// environment form with libraries
// ============================================================================

#[test]
fn test_environment_with_scheme_base() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    let result = eval.eval_str("(environment '(scheme base))").unwrap();
    assert!(
        matches!(lisp.get(result).unwrap(), Value::Environment { mutable: false, .. }),
        "environment should return an immutable environment"
    );
}

#[test]
fn test_eval_in_library_environment() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Pre-create the environment, then use it in eval
    eval.eval_str("(import (scheme base))").unwrap();
    eval.eval_str("(define base-env (environment '(scheme base)))").unwrap();
    assert_eq!(
        eval_to_num(&lisp, &mut eval, "(eval '(+ 2 3) base-env)"),
        5
    );
}

// ============================================================================
// (scheme r5rs) library
// ============================================================================

#[test]
fn test_import_scheme_r5rs() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // (scheme r5rs) should auto-load from embedded sources
    eval.eval_str("(import (scheme r5rs))").unwrap();
}

#[test]
fn test_scheme_r5rs_arithmetic() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(+ 1 2 3)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(* 4 5)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(abs -7)"), 7);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(max 3 5 1)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(min 3 5 1)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(gcd 12 8)"), 4);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(quotient 10 3)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(modulo 10 3)"), 1);
}

#[test]
fn test_scheme_r5rs_list_operations() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(car '(1 2 3))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(cadr '(1 2 3))"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(length '(1 2 3))"), 3);
    assert!(eval_is_true(&lisp, &mut eval, "(list? '(1 2))"));
    assert_eq!(eval_to_num(&lisp, &mut eval, "(list-ref '(10 20 30) 1)"), 20);
}

#[test]
fn test_scheme_r5rs_macros() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    // let
    assert_eq!(eval_to_num(&lisp, &mut eval, "(let ((x 3)) x)"), 3);
    // cond
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(cond (#f 1) (#t 2) (#t 3))"), 2);
    // and / or
    assert!(eval_is_true(&lisp, &mut eval, "(and #t #t)"));
    assert!(eval_is_true(&lisp, &mut eval, "(or #f #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(and #t #f)"));
}

#[test]
fn test_scheme_r5rs_exact_inexact_names() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    // R5RS names: exact->inexact and inexact->exact
    assert!(eval_is_true(&lisp, &mut eval, "(inexact? (exact->inexact 5))"));
    assert!(eval_is_true(&lisp, &mut eval, "(exact? (inexact->exact 5.0))"));
}

#[test]
fn test_scheme_r5rs_string_operations() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(string=? \"hello\" \"hello\")"));
    assert!(eval_is_false(&lisp, &mut eval, "(string=? \"hello\" \"world\")"));
    assert_eq!(eval_to_num(&lisp, &mut eval, "(string-length \"hello\")"), 5);
}

#[test]
fn test_scheme_r5rs_char_operations() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(char-alphabetic? #\\a)"));
    assert!(eval_is_false(&lisp, &mut eval, "(char-alphabetic? #\\1)"));
    assert!(eval_is_true(&lisp, &mut eval, "(char-ci=? #\\A #\\a)"));
}

#[test]
fn test_scheme_r5rs_vector_operations() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-ref (vector 10 20 30) 1)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(vector-length (vector 1 2 3))"), 3);
    assert!(eval_is_true(&lisp, &mut eval, "(vector? (vector 1))"));
}

#[test]
fn test_scheme_r5rs_no_transcript() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme r5rs))").unwrap();
    // transcript-on and transcript-off should NOT be available
    // They're not implemented, so calling them should error
    assert!(eval.eval_str("(transcript-on \"log.txt\")").is_err());
    assert!(eval.eval_str("(transcript-off)").is_err());
}

// ============================================================================
// (scheme complex) library
// ============================================================================

#[test]
fn test_import_scheme_complex() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme complex))").unwrap();
    // real-part and imag-part should work on real numbers
    assert_eq!(eval_to_num(&lisp, &mut eval, "(real-part 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(imag-part 5)"), 0);
}

// ============================================================================
// (scheme base) new exports
// ============================================================================

#[test]
fn test_scheme_base_bytevector_exports() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme base))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(bytevector? (make-bytevector 3))"));
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length (make-bytevector 5))"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (bytevector 10 20 30) 1)"), 20);
}

#[test]
fn test_scheme_base_io_exports() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme base))").unwrap();
    // write-string and flush-output-port should be available
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? write-string)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? flush-output-port)"));
    // call-with-port should be available
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? call-with-port)"));
}

#[test]
fn test_scheme_base_features_export() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme base))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(list? (features))"));
}

#[test]
fn test_scheme_base_special_forms_work() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme base))").unwrap();
    // apply should work
    assert_eq!(eval_to_num(&lisp, &mut eval, "(apply + '(1 2 3))"), 6);
    // values and call-with-values should work
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(call-with-values (lambda () (values 1 2)) +)"), 3);
}

#[test]
fn test_scheme_base_error_predicates() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme base))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? read-error?)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? file-error?)"));
}

#[test]
fn test_scheme_base_encoding_exports() {
    let lisp: Lisp<30000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme base))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? utf8->string)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? string->utf8)"));
}

// ============================================================================
// (scheme cxr) new 4-level exports
// ============================================================================

#[test]
fn test_scheme_cxr_4level() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme cxr))").unwrap();
    // caaaar: (car (car (car (car x))))
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(caaaar '((((1 2) 3) 4) 5))"), 1);
    // caddar: (car (cdr (cdr (car x))))
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(caddar '((a b 3) d))"), 3);
}

// ============================================================================
// Test new cxr functions directly (no import needed)
// ============================================================================

#[test]
fn test_cxr_new_4level_functions() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // cdaddr: (cdr (car (cdr (cdr x))))
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(car (cdaddr '(a b (c 3) d)))"), 3);
    // cddaar: (cdr (cdr (car (car x))))
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(car (cddaar '(((a b 3) c) d)))"), 3);
    // cddadr: (cdr (cdr (car (cdr x))))
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(car (cddadr '(a (b c 3) d)))"), 3);
    // cdddar: (cdr (cdr (cdr (car x))))
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(car (cdddar '((a b c 3) d)))"), 3);
}

// ============================================================================
// (scheme file) new exports
// ============================================================================

#[test]
fn test_scheme_file_exports() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme file))").unwrap();
    // These should all be available as procedures
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? open-input-file)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? open-output-file)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? call-with-input-file)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? call-with-output-file)"));
}

// ============================================================================
// (scheme time) new exports
// ============================================================================

#[test]
fn test_scheme_time_exports() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    eval.eval_str("(import (scheme time))").unwrap();
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? current-second)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? current-jiffy)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? jiffies-per-second)"));
}

// ============================================================================
// Error cases
// ============================================================================

#[test]
fn test_import_unknown_library_fails() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    let result = eval.eval_str("(import (nonexistent lib))");
    assert!(result.is_err(), "Importing a non-existent library should fail");
}

// ============================================================================
// (scheme inexact) R7RS names: exact, inexact
// ============================================================================

#[test]
fn test_scheme_inexact_r7rs_names() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // R7RS names exact and inexact should be available as builtins
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? exact)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? inexact)"));
    assert_eq!(eval_to_num(&lisp, &mut eval, "(exact 3.0)"), 3);
}

// ============================================================================
// (scheme base) char comparison exports
// ============================================================================

#[test]
fn test_scheme_base_char_comparison_exports() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // These should be available from base without explicit import
    assert!(eval_is_true(&lisp, &mut eval, "(char=? #\\a #\\a)"));
    assert!(eval_is_true(&lisp, &mut eval, "(char<? #\\a #\\b)"));
    assert!(eval_is_true(&lisp, &mut eval, "(char>? #\\b #\\a)"));
    assert!(eval_is_true(&lisp, &mut eval, "(char<=? #\\a #\\b)"));
    assert!(eval_is_true(&lisp, &mut eval, "(char>=? #\\b #\\a)"));
}

// ============================================================================
// (scheme base) read-bytevector exports
// ============================================================================

#[test]
fn test_scheme_base_read_bytevector_exports() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // read-bytevector and read-bytevector! should be available as procedures
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? read-bytevector)"));
    assert!(eval_is_true(&lisp, &mut eval, "(procedure? read-bytevector!)"));
}


