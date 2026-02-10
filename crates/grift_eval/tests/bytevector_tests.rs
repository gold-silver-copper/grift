mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_is_false, eval_to_string};

// ============================================================================
// bytevector? predicate
// ============================================================================

#[test]
fn test_bytevector_predicate() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(bytevector? #u8(1 2 3))"));
    assert!(eval_is_true(&lisp, &mut eval, "(bytevector? (make-bytevector 0))"));
    assert!(eval_is_true(&lisp, &mut eval, "(bytevector? (make-bytevector 5))"));
    assert!(eval_is_false(&lisp, &mut eval, "(bytevector? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(bytevector? \"hello\")"));
    assert!(eval_is_false(&lisp, &mut eval, "(bytevector? '(1 2 3))"));
    assert!(eval_is_false(&lisp, &mut eval, "(bytevector? #t)"));
}

// ============================================================================
// make-bytevector
// ============================================================================

#[test]
fn test_make_bytevector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Default fill is 0
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (make-bytevector 3) 0)"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (make-bytevector 3) 2)"), 0);

    // Custom fill
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (make-bytevector 3 42) 0)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (make-bytevector 3 42) 1)"), 42);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (make-bytevector 3 42) 2)"), 42);

    // Edge: fill 255
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref (make-bytevector 1 255) 0)"), 255);
}

#[test]
fn test_make_bytevector_empty() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length (make-bytevector 0))"), 0);
}

// ============================================================================
// bytevector-length
// ============================================================================

#[test]
fn test_bytevector_length() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length #u8())"), 0);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length #u8(1))"), 1);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length #u8(1 2 3))"), 3);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-length (make-bytevector 5))"), 5);
}

// ============================================================================
// bytevector-u8-ref
// ============================================================================

#[test]
fn test_bytevector_u8_ref() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref #u8(10 20 30) 0)"), 10);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref #u8(10 20 30) 1)"), 20);
    assert_eq!(eval_to_num(&lisp, &mut eval, "(bytevector-u8-ref #u8(10 20 30) 2)"), 30);
}

// ============================================================================
// bytevector-u8-set!
// ============================================================================

#[test]
fn test_bytevector_u8_set() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let ((bv (make-bytevector 3 0))) (bytevector-u8-set! bv 1 99) (bytevector-u8-ref bv 1))"),
        99);
    // Verify other elements unchanged
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let ((bv (make-bytevector 3 0))) (bytevector-u8-set! bv 1 99) (bytevector-u8-ref bv 0))"),
        0);
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let ((bv (make-bytevector 3 0))) (bytevector-u8-set! bv 1 99) (bytevector-u8-ref bv 2))"),
        0);
}

// ============================================================================
// bytevector-copy
// ============================================================================

#[test]
fn test_bytevector_copy() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Full copy
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-copy #u8(1 2 3))"), "#u8(1 2 3)");

    // Copy with start
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-copy #u8(1 2 3 4 5) 2)"), "#u8(3 4 5)");

    // Copy with start and end
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-copy #u8(1 2 3 4 5) 1 3)"), "#u8(2 3)");

    // Empty range
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-copy #u8(1 2 3) 2 2)"), "#u8()");
}

#[test]
fn test_bytevector_copy_independence() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Verify copy is independent of original
    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(let ((bv #u8(1 2 3)) (bv2 (bytevector-copy #u8(1 2 3)))) (bytevector-u8-set! bv2 0 99) (bytevector-u8-ref bv 0))"),
        1);
}

// ============================================================================
// bytevector-append
// ============================================================================

#[test]
fn test_bytevector_append() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Append two bytevectors
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-append #u8(1 2) #u8(3 4))"), "#u8(1 2 3 4)");

    // Append three bytevectors
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-append #u8(1) #u8(2) #u8(3))"), "#u8(1 2 3)");

    // Append with empty
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-append #u8() #u8(1 2))"), "#u8(1 2)");
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-append #u8(1 2) #u8())"), "#u8(1 2)");

    // No arguments
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(bytevector-append)"), "#u8()");
}

// ============================================================================
// utf8->string
// ============================================================================

#[test]
fn test_utf8_to_string() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // ASCII
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(utf8->string #u8(104 101 108 108 111))"), "\"hello\"");

    // Empty
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(utf8->string #u8())"), "\"\"");

    // With range
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(utf8->string #u8(104 101 108 108 111) 1 4)"), "\"ell\"");
}

#[test]
fn test_utf8_to_string_multibyte() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // UTF-8 multibyte: λ is U+03BB, encoded as 0xCE 0xBB
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(utf8->string #u8(206 187))"), "\"λ\"");
}

// ============================================================================
// string->utf8
// ============================================================================

#[test]
fn test_string_to_utf8() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // ASCII
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(string->utf8 \"hello\")"), "#u8(104 101 108 108 111)");

    // Empty
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(string->utf8 \"\")"), "#u8()");

    // With range
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(string->utf8 \"hello\" 1 4)"), "#u8(101 108 108)");
}

#[test]
fn test_string_to_utf8_multibyte() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Round-trip: decode UTF-8 bytes to string, then re-encode
    // λ is U+03BB, encoded as 0xCE 0xBB
    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(string->utf8 (utf8->string #u8(206 187)))"), "#u8(206 187)");
}

// ============================================================================
// Round-trip: string->utf8 -> utf8->string
// ============================================================================

#[test]
fn test_utf8_roundtrip() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(utf8->string (string->utf8 \"hello\"))"), "\"hello\"");
}

// ============================================================================
// bytevector-append with no args produces empty bytevector
// ============================================================================

#[test]
fn test_bytevector_append_no_args() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval,
        "(bytevector-length (bytevector-append))"), 0);
}

// ============================================================================
// Bytevector literal display
// ============================================================================

#[test]
fn test_bytevector_literal_display() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval, "#u8(0 1 2)"), "#u8(0 1 2)");
    assert_eq!(eval_to_string(&lisp, &mut eval, "#u8()"), "#u8()");
}
