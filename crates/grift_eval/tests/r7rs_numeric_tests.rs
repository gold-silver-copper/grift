//! Tests for R7RS §6.2.6 numeric procedures:
//! - Type predicates: complex?, real?, rational?, integer?
//! - Division procedures: floor/, floor-quotient, floor-remainder,
//!   truncate/, truncate-quotient, truncate-remainder
//! - Rational operations: numerator, denominator, rationalize
//! - Exact integer square root: exact-integer-sqrt
//! - Transcendental functions: exp, log, sin, cos, tan, asin, acos, atan
//! - Complex number operations: make-rectangular, make-polar, real-part,
//!   imag-part, magnitude, angle
//! - Number-string conversion with radix: number->string, string->number

mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_is_false, eval_to_string};

// ============================================================================
// Type Predicates (R7RS §6.2.6)
// ============================================================================

#[test]
fn test_complex_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(complex? 3)"));
    assert!(eval_is_true(&lisp, &mut eval, "(complex? 3.5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(complex? 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(complex? \"hello\")"));
    assert!(eval_is_false(&lisp, &mut eval, "(complex? #t)"));
}

#[test]
fn test_real_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(real? 3)"));
    assert!(eval_is_true(&lisp, &mut eval, "(real? 3.5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(real? -1)"));
    assert!(eval_is_false(&lisp, &mut eval, "(real? \"hello\")"));
}

#[test]
fn test_rational_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(rational? 3)"));
    assert!(eval_is_true(&lisp, &mut eval, "(rational? 3.5)"));
    assert!(eval_is_true(&lisp, &mut eval, "(rational? 0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(rational? \"hello\")"));
}

#[test]
fn test_integer_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(integer? 3)"));
    assert!(eval_is_true(&lisp, &mut eval, "(integer? 3.0)"));
    assert!(eval_is_false(&lisp, &mut eval, "(integer? 3.5)"));
    assert!(eval_is_false(&lisp, &mut eval, "(integer? \"hello\")"));
}

// ============================================================================
// Division Procedures (R7RS §6.2.6)
// ============================================================================

#[test]
fn test_floor_quotient() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-quotient 5 2)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-quotient -5 2)"), -3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-quotient 5 -2)"), -3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-quotient -5 -2)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-quotient 10 5)"), 2);
}

#[test]
fn test_floor_remainder() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-remainder 5 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-remainder -5 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-remainder 5 -2)"), -1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(floor-remainder -5 -2)"), -1);
}

#[test]
fn test_floor_div() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // floor/ returns values as a list
    assert_eq!(eval_to_string(&lisp, &mut eval, "(floor/ 5 2)"), "(2 1)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(floor/ -5 2)"), "(-3 1)");
}

#[test]
fn test_truncate_quotient() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-quotient 5 2)"), 2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-quotient -5 2)"), -2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-quotient 5 -2)"), -2);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-quotient -5 -2)"), 2);
}

#[test]
fn test_truncate_remainder() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-remainder 5 2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-remainder -5 2)"), -1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-remainder 5 -2)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(truncate-remainder -5 -2)"), -1);
}

#[test]
fn test_truncate_div() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(truncate/ 5 2)"), "(2 1)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(truncate/ -5 2)"), "(-2 -1)");
}

#[test]
fn test_division_invariant() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // n = d·q + r must hold
    assert!(eval_is_true(&lisp, &mut eval,
        "(let ((q (floor-quotient 7 3)) (r (floor-remainder 7 3))) (= 7 (+ (* 3 q) r)))"));
    assert!(eval_is_true(&lisp, &mut eval,
        "(let ((q (floor-quotient -7 3)) (r (floor-remainder -7 3))) (= -7 (+ (* 3 q) r)))"));
    assert!(eval_is_true(&lisp, &mut eval,
        "(let ((q (truncate-quotient 7 3)) (r (truncate-remainder 7 3))) (= 7 (+ (* 3 q) r)))"));
    assert!(eval_is_true(&lisp, &mut eval,
        "(let ((q (truncate-quotient -7 3)) (r (truncate-remainder -7 3))) (= -7 (+ (* 3 q) r)))"));
}

// ============================================================================
// Rational Number Operations (R7RS §6.2.6)
// ============================================================================

#[test]
fn test_numerator_exact() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(numerator 6)"), 6);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(numerator 0)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(numerator -3)"), -3);
}

#[test]
fn test_denominator_exact() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(denominator 6)"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(denominator 0)"), 1);
}

#[test]
fn test_numerator_inexact() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // 0.5 = 1/2, so numerator should be 1.0
    assert!(eval_is_true(&lisp, &mut eval, "(= (numerator 0.5) 1.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(inexact? (numerator 0.5))"));
}

#[test]
fn test_denominator_inexact() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // 0.5 = 1/2, so denominator should be 2.0
    assert!(eval_is_true(&lisp, &mut eval, "(= (denominator 0.5) 2.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(inexact? (denominator 0.5))"));
}

#[test]
fn test_rationalize() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // rationalize 0 within any tolerance should be 0
    assert_eq!(eval_to_num(&lisp, &mut eval, "(rationalize 0 1)"), 0);
}

// ============================================================================
// Exact Integer Square Root (R7RS §6.2.6)
// ============================================================================

#[test]
fn test_exact_integer_sqrt() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(exact-integer-sqrt 4)"), "(2 0)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(exact-integer-sqrt 5)"), "(2 1)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(exact-integer-sqrt 0)"), "(0 0)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(exact-integer-sqrt 1)"), "(1 0)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(exact-integer-sqrt 15)"), "(3 6)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(exact-integer-sqrt 16)"), "(4 0)");
}

// ============================================================================
// Transcendental Functions (R7RS §6.2.6)
// ============================================================================

#[test]
fn test_exp() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (exp 0) 1.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(> (exp 1) 2.718)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (exp 1) 2.719)"));
}

#[test]
fn test_log() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // log(1) = 0
    assert!(eval_is_true(&lisp, &mut eval, "(= (log 1) 0.0)"));
    // log(e) ≈ 1
    assert!(eval_is_true(&lisp, &mut eval, "(< (abs (- (log (exp 1)) 1.0)) 0.0001)"));
}

#[test]
fn test_log_with_base() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // log(100, 10) = 2
    assert!(eval_is_true(&lisp, &mut eval, "(< (abs (- (log 100 10) 2.0)) 0.0001)"));
    // log(8, 2) = 3
    assert!(eval_is_true(&lisp, &mut eval, "(< (abs (- (log 8 2) 3.0)) 0.0001)"));
}

#[test]
fn test_sin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (sin 0) 0.0)"));
    assert!(eval_is_true(&lisp, &mut eval, "(> (sin 1) 0.84)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (sin 1) 0.85)"));
}

#[test]
fn test_cos() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (cos 0) 1.0)"));
}

#[test]
fn test_tan() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (tan 0) 0.0)"));
}

#[test]
fn test_asin() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // asin(0) = 0
    assert!(eval_is_true(&lisp, &mut eval, "(= (asin 0) 0.0)"));
    // asin(0.5) ≈ π/6 ≈ 0.5236
    assert!(eval_is_true(&lisp, &mut eval, "(> (asin 0.5) 0.523)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (asin 0.5) 0.524)"));
}

#[test]
fn test_acos() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // acos(1) = 0
    assert!(eval_is_true(&lisp, &mut eval, "(= (acos 1) 0.0)"));
}

#[test]
fn test_atan_one_arg() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // atan(0) = 0
    assert!(eval_is_true(&lisp, &mut eval, "(= (atan 0) 0.0)"));
    // atan(1) ≈ π/4 ≈ 0.7854
    assert!(eval_is_true(&lisp, &mut eval, "(> (atan 1) 0.785)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (atan 1) 0.786)"));
}

#[test]
fn test_atan_two_args() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // atan(1, 1) ≈ π/4
    assert!(eval_is_true(&lisp, &mut eval, "(> (atan 1 1) 0.785)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (atan 1 1) 0.786)"));
}

// ============================================================================
// Complex Number Operations (R7RS §6.2.6)
// ============================================================================

#[test]
fn test_make_rectangular() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Pure real
    assert!(eval_is_true(&lisp, &mut eval, "(= (make-rectangular 3 0) 3.0)"));

    // Complex with non-zero imaginary (returns a native complex value, not a pair)
    assert!(eval_is_true(&lisp, &mut eval, "(number? (make-rectangular 3 4))"));
}

#[test]
fn test_real_part() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Real number
    assert_eq!(eval_to_num(&lisp, &mut eval, "(real-part 5)"), 5);
    // Complex
    assert!(eval_is_true(&lisp, &mut eval, "(= (real-part (make-rectangular 3 4)) 3.0)"));
}

#[test]
fn test_imag_part() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Real number has zero imaginary part
    assert_eq!(eval_to_num(&lisp, &mut eval, "(imag-part 5)"), 0);
    // Complex
    assert!(eval_is_true(&lisp, &mut eval, "(= (imag-part (make-rectangular 3 4)) 4.0)"));
}

#[test]
fn test_magnitude() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Real number magnitude is absolute value
    assert_eq!(eval_to_num(&lisp, &mut eval, "(magnitude 5)"), 5);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(magnitude -5)"), 5);
    // Complex 3+4i has magnitude 5
    assert!(eval_is_true(&lisp, &mut eval, "(= (magnitude (make-rectangular 3 4)) 5.0)"));
}

#[test]
fn test_angle() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Positive real number has angle 0
    assert!(eval_is_true(&lisp, &mut eval, "(= (angle 5) 0.0)"));
    // 1+1i has angle π/4
    assert!(eval_is_true(&lisp, &mut eval, "(> (angle (make-rectangular 1 1)) 0.785)"));
    assert!(eval_is_true(&lisp, &mut eval, "(< (angle (make-rectangular 1 1)) 0.786)"));
}

#[test]
fn test_make_polar() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // make-polar with angle 0 gives real number
    assert!(eval_is_true(&lisp, &mut eval, "(= (make-polar 5 0) 5.0)"));
    // make-polar (5, ~0.927) ≈ 3+4i
    assert!(eval_is_true(&lisp, &mut eval,
        "(< (abs (- (real-part (make-polar 5 0.9272952180016122)) 3.0)) 0.001)"));
}

// ============================================================================
// Number-String Conversion with Radix (R7RS §6.2.6)
// ============================================================================

#[test]
fn test_number_to_string_decimal() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string 255)"), "\"255\"");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string 0)"), "\"0\"");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string -42)"), "\"-42\"");
}

#[test]
fn test_number_to_string_hex() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string 255 16)"), "\"ff\"");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string 16 16)"), "\"10\"");
}

#[test]
fn test_number_to_string_binary() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string 255 2)"), "\"11111111\"");
    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string 10 2)"), "\"1010\"");
}

#[test]
fn test_number_to_string_octal() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "(number->string 255 8)"), "\"377\"");
}

#[test]
fn test_string_to_number_decimal() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(string->number \"255\")"), 255);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(string->number \"-42\")"), -42);
}

#[test]
fn test_string_to_number_hex() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(string->number \"ff\" 16)"), 255);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(string->number \"FF\" 16)"), 255);
}

#[test]
fn test_string_to_number_prefix() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(string->number \"#xff\")"), 255);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(string->number \"#b1111\")"), 15);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(string->number \"#o377\")"), 255);
}

#[test]
fn test_string_to_number_invalid() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_false(&lisp, &mut eval, "(string->number \"hello\")"));
    assert!(eval_is_false(&lisp, &mut eval, "(string->number \"\")"));
}

#[test]
fn test_string_number_roundtrip() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(= (string->number (number->string 42)) 42)"));
    assert!(eval_is_true(&lisp, &mut eval, "(= (string->number (number->string 255 16) 16) 255)"));
}

// ============================================================================
// Complex predicates with tagged complex values
// ============================================================================

#[test]
fn test_complex_predicate_with_tagged() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(complex? (make-rectangular 3 4))"));
    assert!(eval_is_true(&lisp, &mut eval, "(complex? 3)"));
}
