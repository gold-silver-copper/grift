mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_is_false};

// ============================================================================
// File System Operations (R7RS §6.13)
// ============================================================================

#[test]
fn test_file_exists_true() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Cargo.toml should always exist in the workspace root
    assert!(eval_is_true(&lisp, &mut eval, "(file-exists? \"Cargo.toml\")"));
}

#[test]
fn test_file_exists_false() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    assert!(eval_is_false(&lisp, &mut eval, "(file-exists? \"nonexistent_file_12345.txt\")"));
}

#[test]
fn test_delete_file() {
    use std::fs;
    use std::io::Write;

    // Create a temp file to delete
    let path = "/tmp/grift_test_delete_file.txt";
    {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(b"test").unwrap();
    }
    assert!(std::path::Path::new(path).exists());

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!("(delete-file \"{}\")", path);
    eval.eval_str(&expr).unwrap();

    assert!(!std::path::Path::new(path).exists());
}

#[test]
fn test_delete_file_nonexistent_raises_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let result = eval.eval_str("(delete-file \"nonexistent_file_xyz_12345.txt\")");
    assert!(result.is_err());
}

#[test]
fn test_load_file() {
    use std::fs;
    use std::io::Write;

    // Create a temp Scheme file to load
    let path = "/tmp/grift_test_load.scm";
    {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(b"(define grift-load-test-var 42)").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!("(load \"{}\")", path);
    eval.eval_str(&expr).unwrap();

    // The loaded file should have defined the variable
    assert_eq!(eval_to_num(&lisp, &mut eval, "grift-load-test-var"), 42);

    // Clean up
    let _ = fs::remove_file(path);
}

#[test]
fn test_load_file_multiple_expressions() {
    use std::fs;
    use std::io::Write;

    let path = "/tmp/grift_test_load_multi.scm";
    {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(b"(define x-load-test 10)\n(define y-load-test 20)\n(define z-load-test (+ x-load-test y-load-test))").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!("(load \"{}\")", path);
    eval.eval_str(&expr).unwrap();

    assert_eq!(eval_to_num(&lisp, &mut eval, "z-load-test"), 30);

    let _ = fs::remove_file(path);
}

#[test]
fn test_load_nonexistent_file_raises_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let result = eval.eval_str("(load \"nonexistent_file_xyz_12345.scm\")");
    assert!(result.is_err());
}

// ============================================================================
// Process / Environment Operations (R7RS §6.14)
// ============================================================================

#[test]
fn test_command_line_returns_list() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // command-line should return a list (at least one element: the program name)
    assert!(eval_is_true(&lisp, &mut eval, "(pair? (command-line))"));
    // The first element should be a string
    assert!(eval_is_true(&lisp, &mut eval, "(string? (car (command-line)))"));
}

#[test]
fn test_get_environment_variable_exists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // PATH should exist on most systems
    // Check that it returns a string (not #f)
    assert!(eval_is_true(&lisp, &mut eval, "(string? (get-environment-variable \"PATH\"))"));
}

#[test]
fn test_get_environment_variable_not_exists() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // A very unlikely variable name
    assert!(eval_is_false(&lisp, &mut eval, "(get-environment-variable \"GRIFT_NONEXISTENT_VAR_XYZ_12345\")"));
}

#[test]
fn test_get_environment_variables_returns_list() {
    let lisp: Lisp<50000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Should return a list (there's always at least one env var)
    assert!(eval_is_true(&lisp, &mut eval, "(pair? (get-environment-variables))"));
    // Each element should be a pair
    assert!(eval_is_true(&lisp, &mut eval, "(pair? (car (get-environment-variables)))"));
    // The car of each pair should be a string (the variable name)
    assert!(eval_is_true(&lisp, &mut eval, "(string? (car (car (get-environment-variables))))"));
    // The cdr of each pair should be a string (the variable value)
    assert!(eval_is_true(&lisp, &mut eval, "(string? (cdr (car (get-environment-variables))))"));
}

// Note: exit and emergency-exit are not tested here because they would
// terminate the test process. They are tested implicitly by their
// compilation and registration as builtins.
