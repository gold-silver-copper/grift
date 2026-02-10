//! Tests for newly implemented R7RS procedures:
//! - Time procedures (§6.13.3): current-second, current-jiffy, jiffies-per-second
//! - Error predicates (§6.11): read-error?, file-error?
//! - Vector-String conversion (§6.8): vector->string, string->vector
//! - Include forms (§4.1.7): include, include-ci
//! - Features (§6.13.3): features procedure

mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_is_false, eval_to_string};

// ============================================================================
// Time Procedures (R7RS §6.13.3)
// ============================================================================

#[test]
fn test_current_second_returns_positive_float() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // current-second should return an inexact positive number
    assert!(eval_is_true(&lisp, &mut eval, "(inexact? (current-second))"));
    assert!(eval_is_true(&lisp, &mut eval, "(> (current-second) 0)"));
}

#[test]
fn test_current_second_reasonable_epoch() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Should be after 2020-01-01 (epoch 1577836800) and a number
    assert!(eval_is_true(&lisp, &mut eval, "(number? (current-second))"));
    assert!(eval_is_true(&lisp, &mut eval, "(> (current-second) 1577836800)"));
}

#[test]
fn test_current_jiffy_returns_exact_integer() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // current-jiffy should return an exact positive integer
    assert!(eval_is_true(&lisp, &mut eval, "(exact? (current-jiffy))"));
    assert!(eval_is_true(&lisp, &mut eval, "(> (current-jiffy) 0)"));
}

#[test]
fn test_current_jiffy_monotonic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Two calls should return non-decreasing values
    assert!(eval_is_true(&lisp, &mut eval,
        "(let ((j1 (current-jiffy))) (<= j1 (current-jiffy)))"));
}

#[test]
fn test_jiffies_per_second_exact_positive() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // jiffies-per-second should return an exact positive integer
    assert!(eval_is_true(&lisp, &mut eval, "(exact? (jiffies-per-second))"));
    assert!(eval_is_true(&lisp, &mut eval, "(> (jiffies-per-second) 0)"));
    // We use nanoseconds, so should be 1000000000
    assert_eq!(eval_to_num(&lisp, &mut eval, "(jiffies-per-second)"), 1_000_000_000);
}

// ============================================================================
// Error Predicates (R7RS §6.11)
// ============================================================================

#[test]
fn test_read_error_p_returns_false_for_non_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_false(&lisp, &mut eval, "(read-error? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(read-error? \"hello\")"));
    assert!(eval_is_false(&lisp, &mut eval, "(read-error? #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(read-error? '())"));
}

#[test]
fn test_file_error_p_returns_false_for_non_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_false(&lisp, &mut eval, "(file-error? 42)"));
    assert!(eval_is_false(&lisp, &mut eval, "(file-error? \"hello\")"));
    assert!(eval_is_false(&lisp, &mut eval, "(file-error? #t)"));
    assert!(eval_is_false(&lisp, &mut eval, "(file-error? '())"));
}

#[test]
fn test_read_error_p_returns_false_for_regular_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // A regular error object (not a read error) should return #f
    assert!(eval_is_false(&lisp, &mut eval,
        r#"(guard (e (#t (read-error? e))) (error "test" 1))"#));
}

#[test]
fn test_file_error_p_recognizes_file_errors() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Opening a non-existent file should raise a file-error
    assert!(eval_is_true(&lisp, &mut eval,
        r#"(guard (e (#t (file-error? e))) (open-input-file "/tmp/grift_nonexistent_12345.txt"))"#));
}

#[test]
fn test_read_error_p_recognizes_parse_errors() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Reading invalid syntax from a string port should raise a read-error
    assert!(eval_is_true(&lisp, &mut eval,
        r#"(guard (e (#t (read-error? e))) (read (open-input-string ")")))"#));
}

#[test]
fn test_file_error_not_read_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // A file error should NOT be a read error
    assert!(eval_is_false(&lisp, &mut eval,
        r#"(guard (e (#t (read-error? e))) (open-input-file "/tmp/grift_nonexistent_12345.txt"))"#));
}

#[test]
fn test_read_error_not_file_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // A read error should NOT be a file error
    assert!(eval_is_false(&lisp, &mut eval,
        r#"(guard (e (#t (file-error? e))) (read (open-input-string ")")))"#));
}

#[test]
fn test_delete_file_nonexistent_is_file_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    assert!(eval_is_true(&lisp, &mut eval,
        r#"(guard (e (#t (file-error? e))) (delete-file "/tmp/grift_nonexistent_12345.txt"))"#));
}

#[test]
fn test_error_predicates_return_false_for_each_other() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Regular error: neither read-error nor file-error
    assert!(eval_is_true(&lisp, &mut eval,
        r#"(guard (e (#t (and (error-object? e) (not (read-error? e)) (not (file-error? e)))))
             (error "test"))"#));
}

// ============================================================================
// Vector-String Conversion (R7RS §6.8)
// ============================================================================

#[test]
fn test_vector_to_string_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(vector->string #(#\h #\e #\l #\l #\o))"#), "\"hello\"");
}

#[test]
fn test_vector_to_string_empty() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        "(vector->string #())"), "\"\"");
}

#[test]
fn test_vector_to_string_with_start() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(vector->string #(#\a #\b #\c #\d) 1)"#), "\"bcd\"");
}

#[test]
fn test_vector_to_string_with_start_end() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(vector->string #(#\a #\b #\c #\d) 1 3)"#), "\"bc\"");
}

#[test]
fn test_string_to_vector_basic() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(string->vector "hello")"#), "#(#\\h #\\e #\\l #\\l #\\o)");
}

#[test]
fn test_string_to_vector_empty() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(string->vector "")"#), "#()");
}

#[test]
fn test_string_to_vector_with_start_end() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(string->vector "abcd" 1 3)"#), "#(#\\b #\\c)");
}

#[test]
fn test_vector_string_roundtrip() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(vector->string (string->vector "hello"))"#), "\"hello\"");
}

#[test]
fn test_string_vector_roundtrip() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert_eq!(eval_to_string(&lisp, &mut eval,
        r#"(string->vector (vector->string #(#\a #\b #\c)))"#), "#(#\\a #\\b #\\c)");
}

#[test]
fn test_vector_to_string_type_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Vector containing non-characters should raise an error
    let result = eval.eval_str("(vector->string #(1 2 3))");
    assert!(result.is_err());
}

#[test]
fn test_vector_to_string_not_a_vector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Non-vector argument should raise an error
    let result = eval.eval_str(r#"(vector->string "hello")"#);
    assert!(result.is_err());
}

#[test]
fn test_string_to_vector_not_a_string() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Non-string argument should raise an error
    let result = eval.eval_str("(string->vector 42)");
    assert!(result.is_err());
}

// ============================================================================
// Include Forms (R7RS §4.1.7)
// ============================================================================

#[test]
fn test_include_basic() {
    use std::fs;
    use std::io::Write;

    let path = std::env::temp_dir().join("grift_test_include.scm");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"(define include-test-var 42)").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(r#"(begin (include "{}") include-test-var)"#,
        path.display());
    assert_eq!(eval_to_num(&lisp, &mut eval, &expr), 42);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_include_multiple_expressions() {
    use std::fs;
    use std::io::Write;

    let path = std::env::temp_dir().join("grift_test_include_multi.scm");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"(define inc-a 10)\n(define inc-b 20)").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(r#"(begin (include "{}") (+ inc-a inc-b))"#,
        path.display());
    assert_eq!(eval_to_num(&lisp, &mut eval, &expr), 30);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_include_missing_file_errors() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Including a nonexistent file should raise an error
    let result = eval.eval_str(r#"(include "/tmp/grift_nonexistent_file_12345.scm")"#);
    assert!(result.is_err());
}

// ============================================================================
// Features Procedure (R7RS §6.13.3) - verify existing implementation
// ============================================================================

#[test]
fn test_features_is_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(list? (features))"));
}

#[test]
fn test_features_contains_r7rs() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(pair? (memq 'r7rs (features)))"));
}

#[test]
fn test_features_contains_grift() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    assert!(eval_is_true(&lisp, &mut eval, "(pair? (memq 'grift (features)))"));
}
