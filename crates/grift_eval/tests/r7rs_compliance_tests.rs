mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_is_false, eval_to_string};

// ============================================================================
// map with multiple lists (R7RS §6.4)
// ============================================================================

#[test]
fn test_map_single_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(map + '(1 2 3))"), "(1 2 3)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(map car '((1 2) (3 4)))"), "(1 3)");
}

#[test]
fn test_map_two_lists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(map + '(1 2 3) '(10 20 30))"), "(11 22 33)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(map * '(2 3) '(4 5))"), "(8 15)");
}

#[test]
fn test_map_three_lists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(map + '(1 2 3) '(10 20 30) '(100 200 300))"), "(111 222 333)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(map list '(1 2) '(3 4) '(5 6))"), "((1 3 5) (2 4 6))");
}

#[test]
fn test_map_four_lists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(map + '(1) '(2) '(3) '(4))"), "(10)");
}

#[test]
fn test_map_unequal_length_uses_shortest() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(map + '(1 2 3) '(10 20))"), "(11 22)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(map + '(1) '(10 20 30))"), "(11)");
}

// ============================================================================
// for-each with multiple lists (R7RS §6.4)
// ============================================================================

#[test]
fn test_for_each_single_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let ((sum 0)) (for-each (lambda (x) (set! sum (+ sum x))) '(1 2 3)) sum)"), 6);
}

#[test]
fn test_for_each_two_lists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let ((sum 0)) (for-each (lambda (x y) (set! sum (+ sum x y))) '(1 2 3) '(10 20 30)) sum)"), 66);
}

#[test]
fn test_for_each_three_lists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let ((sum 0)) (for-each (lambda (x y z) (set! sum (+ sum x y z))) '(1 2) '(10 20) '(100 200)) sum)"), 333);
}

// ============================================================================
// bytevector constructor (R7RS §6.9)
// ============================================================================

#[test]
fn test_bytevector_constructor() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(bytevector? (bytevector))"));
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length (bytevector))"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length (bytevector 1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (bytevector 10 20 30) 0)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (bytevector 10 20 30) 1)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (bytevector 10 20 30) 2)"), 30);
}

#[test]
fn test_bytevector_constructor_edge_cases() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Single byte
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (bytevector 255) 0)"), 255);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (bytevector 0) 0)"), 0);
}

// ============================================================================
// bytevector-copy! (R7RS §6.9)
// ============================================================================

#[test]
fn test_bytevector_copy_bang_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Copy entire source to destination at position 0
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(let ((to (make-bytevector 5 0)) (from (bytevector 1 2 3))) (bytevector-copy! to 0 from) to)"),
        "#u8(1 2 3 0 0)");
}

#[test]
fn test_bytevector_copy_bang_with_offset() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Copy with offset in destination
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(let ((to (make-bytevector 5 0)) (from (bytevector 1 2 3))) (bytevector-copy! to 1 from) to)"),
        "#u8(0 1 2 3 0)");
}

#[test]
fn test_bytevector_copy_bang_with_range() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Copy subrange of source
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(let ((to (make-bytevector 5 0)) (from (bytevector 1 2 3 4 5))) (bytevector-copy! to 1 from 1 3) to)"),
        "#u8(0 2 3 0 0)");
}

// ============================================================================
// scheme-report-environment and null-environment (R7RS §6.12)
// ============================================================================

#[test]
fn test_scheme_report_environment() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Returns an environment value
    assert_eq!(eval_to_string(&lisp, &mut eval, "(scheme-report-environment 5)"), "#<environment>");
}

#[test]
fn test_null_environment() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(null-environment 5)"), "#<environment>");
}

// ============================================================================
// Rational number literals (R7RS §7.1.1)
// ============================================================================

#[test]
fn test_rational_literals() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "3/4"), "3/4");
    assert_eq!(eval_to_string(&lisp, &mut eval, "-1/2"), "-1/2");
    assert_eq!(eval_to_string(&lisp, &mut eval, "+5/3"), "5/3");
}

#[test]
fn test_rational_normalization() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // 2/4 should normalize to 1/2
    assert_eq!(eval_to_string(&lisp, &mut eval, "2/4"), "1/2");
    // 6/3 should normalize to integer 2
    assert_eq!(eval_to_string(&lisp, &mut eval, "6/3"), "2");
    // 10/5 should normalize to integer 2
    assert_eq!(eval_to_string(&lisp, &mut eval, "10/5"), "2");
}

#[test]
fn test_rational_arithmetic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Addition with rationals promotes to float
    assert!(eval_is_true(&lisp, &mut eval, "(> (+ 1/2 1/3) 0.83)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (+ 1/2 1/3) 0.84)"));
    // Multiplication
    assert!(eval_is_true(&lisp, &mut eval, "(= (* 1/2 2) 1.0)"));
}

#[test]
fn test_rational_exact_inexact() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(exact? 3/4)"));
    assert!(eval_is_true(&lisp, &mut eval, "(inexact? (exact->inexact 3/4))"));
    assert!(eval_is_true(&lisp, &mut eval, "(number? 3/4)"));
}

#[test]
fn test_rational_numerator_denominator() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(numerator 3/4)"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(denominator 3/4)"), 4);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(numerator -1/2)"), -1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(denominator -1/2)"), 2);
}

#[test]
fn test_rational_comparison() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(< 1/4 1/2)"));
    assert!(eval_is_true(&lisp, &mut eval, "(> 3/4 1/2)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (- 1/2 0.5) 0.001)"));
}

// ============================================================================
// Complex number literals (R7RS §7.1.1)
// ============================================================================

#[test]
fn test_complex_rectangular_literals() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(number? 1+2i)"));
    assert!(eval_is_true(&lisp, &mut eval, "(number? 3-4i)"));
}

#[test]
fn test_complex_real_imag_parts() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (real-part 1+2i) 1.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(= (imag-part 1+2i) 2.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(= (real-part 3-4i) 3.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(= (imag-part 3-4i) -4.0)"));
}

#[test]
fn test_complex_magnitude() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (magnitude 3+4i) 5.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(= (magnitude 0+1i) 1.0)"));
}

#[test]
fn test_complex_make_rectangular() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (real-part (make-rectangular 3 4)) 3.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(= (imag-part (make-rectangular 3 4)) 4.0)"));
}

#[test]
fn test_complex_make_polar() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // make-polar with r=5, theta=atan2(4,3) should give 3+4i approximately
    assert!(eval_is_true(&lisp, &mut eval, "(< (- (magnitude (make-polar 5 0.9272952180016122)) 5.0) 0.001)"));
}

// ============================================================================
// member/assoc with optional comparison procedure (R7RS §6.4)
// ============================================================================

#[test]
fn test_member_with_custom_comparator() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // member with default equal?
    assert_eq!(eval_to_string(&lisp, &mut eval, "(member 2 '(1 2 3))"), "(2 3)");
    // member with custom comparator (=)
    assert_eq!(eval_to_string(&lisp, &mut eval, "(member 2.0 '(1 2 3) =)"), "(2 3)");
    // member with custom comparator that never matches
    assert!(eval_is_false(&lisp, &mut eval, "(member 2 '(1 2 3) (lambda (a b) #f))"));
}

#[test]
fn test_assoc_with_custom_comparator() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // assoc with default equal?
    assert_eq!(eval_to_string(&lisp, &mut eval, "(assoc 'b '((a 1) (b 2) (c 3)))"), "(b 2)");
    // assoc with custom comparator (=)
    assert_eq!(eval_to_string(&lisp, &mut eval, "(assoc 2.0 '((1 a) (2 b) (3 c)) =)"), "(2 b)");
    // assoc returns #f when custom comparator never matches
    assert!(eval_is_false(&lisp, &mut eval, "(assoc 2 '((1 a) (2 b)) (lambda (a b) #f))"));
}
