mod common;

use grift_eval::*;
use common::{eval_to_num, eval_is_true, eval_is_false, eval_to_string};

fn temp_path(name: &str) -> String {
    let dir = std::env::temp_dir();
    dir.join(name).to_string_lossy().into_owned()
}

// ============================================================================
// File port operations (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_open_input_file() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_open_input.txt");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"hello world").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-input-file "{}")))
             (let ((c (read-char p)))
               (close-port p)
               c))"#,
        path
    );
    let result = eval.eval_str(&expr).unwrap();
    assert_eq!(lisp.get(result).unwrap(), Value::Char('h'));

    let _ = fs::remove_file(&path);
}

#[test]
fn test_open_output_file() {
    use std::fs;

    let path = temp_path("grift_test_open_output.txt");
    let _ = fs::remove_file(&path);

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-output-file "{}")))
             (write-char #\A p)
             (write-char #\B p)
             (close-port p))"#,
        path
    );
    eval.eval_str(&expr).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "AB");

    let _ = fs::remove_file(&path);
}

#[test]
fn test_open_input_file_nonexistent_raises_error() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let result = eval.eval_str("(open-input-file \"nonexistent_file_xyz_999.txt\")");
    assert!(result.is_err());
}

#[test]
fn test_open_binary_input_file() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_binary_input.bin");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&[65, 66, 67]).unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-input-file "{}")))
             (let ((b (read-u8 p)))
               (close-port p)
               b))"#,
        path
    );
    assert_eq!(eval_to_num(&lisp, &mut eval, &expr), 65);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_open_binary_output_file() {
    use std::fs;

    let path = temp_path("grift_test_binary_output.bin");
    let _ = fs::remove_file(&path);

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-output-file "{}")))
             (write-u8 65 p)
             (write-u8 66 p)
             (close-port p))"#,
        path
    );
    eval.eval_str(&expr).unwrap();

    let contents = fs::read(&path).unwrap();
    assert_eq!(contents, vec![65, 66]);

    let _ = fs::remove_file(&path);
}

// ============================================================================
// call-with-input-file / call-with-output-file (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_call_with_input_file() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_call_input.txt");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"42").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(call-with-input-file "{}" (lambda (p) (read p)))"#,
        path
    );
    assert_eq!(eval_to_num(&lisp, &mut eval, &expr), 42);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_call_with_output_file() {
    use std::fs;

    let path = temp_path("grift_test_call_output.txt");
    let _ = fs::remove_file(&path);

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(call-with-output-file "{}" (lambda (p) (write-string "hello" p)))"#,
        path
    );
    eval.eval_str(&expr).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "hello");

    let _ = fs::remove_file(&path);
}

// ============================================================================
// with-input-from-file / with-output-to-file (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_with_input_from_file() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_with_input.txt");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"99").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(with-input-from-file "{}" (lambda () (read)))"#,
        path
    );
    assert_eq!(eval_to_num(&lisp, &mut eval, &expr), 99);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_with_output_to_file() {
    use std::fs;

    let path = temp_path("grift_test_with_output.txt");
    let _ = fs::remove_file(&path);

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(with-output-to-file "{}" (lambda () (write-string "world")))"#,
        path
    );
    eval.eval_str(&expr).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "world");

    let _ = fs::remove_file(&path);
}

// ============================================================================
// Binary I/O operations (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_read_u8_and_peek_u8() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_read_u8.bin");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&[10, 20, 30]).unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-input-file "{}")))
             (let ((peeked (peek-u8 p)))
               (let ((first (read-u8 p)))
                 (let ((second (read-u8 p)))
                   (close-port p)
                   (list peeked first second)))))"#,
        path
    );
    let result = eval_to_string(&lisp, &mut eval, &expr);
    assert_eq!(result, "(10 10 20)");

    let _ = fs::remove_file(&path);
}

#[test]
fn test_read_u8_eof() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_read_u8_eof.bin");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&[42]).unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-input-file "{}")))
             (read-u8 p)
             (let ((result (eof-object? (read-u8 p))))
               (close-port p)
               result))"#,
        path
    );
    assert!(eval_is_true(&lisp, &mut eval, &expr));

    let _ = fs::remove_file(&path);
}

#[test]
fn test_write_u8() {
    use std::fs;

    let path = temp_path("grift_test_write_u8.bin");
    let _ = fs::remove_file(&path);

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-output-file "{}")))
             (write-u8 255 p)
             (write-u8 0 p)
             (close-port p))"#,
        path
    );
    eval.eval_str(&expr).unwrap();

    let contents = fs::read(&path).unwrap();
    assert_eq!(contents, vec![255, 0]);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_read_bytevector() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_read_bv.bin");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&[1, 2, 3, 4, 5]).unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-input-file "{}")))
             (let ((bv (read-bytevector 3 p)))
               (close-port p)
               (bytevector-length bv)))"#,
        path
    );
    assert_eq!(eval_to_num(&lisp, &mut eval, &expr), 3);

    let _ = fs::remove_file(&path);
}

#[test]
fn test_write_bytevector() {
    use std::fs;

    let path = temp_path("grift_test_write_bv.bin");
    let _ = fs::remove_file(&path);

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-output-file "{}")))
             (write-bytevector #u8(10 20 30) p)
             (close-port p))"#,
        path
    );
    eval.eval_str(&expr).unwrap();

    let contents = fs::read(&path).unwrap();
    assert_eq!(contents, vec![10, 20, 30]);

    let _ = fs::remove_file(&path);
}

// ============================================================================
// Bytevector port operations (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_open_input_bytevector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = r#"(let ((p (open-input-bytevector #u8(10 20 30))))
                    (let ((a (read-u8 p))
                          (b (read-u8 p)))
                      (close-port p)
                      (+ a b)))"#;
    assert_eq!(eval_to_num(&lisp, &mut eval, expr), 30);
}

#[test]
fn test_open_output_bytevector() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = r#"(let ((p (open-output-bytevector)))
                    (write-u8 1 p)
                    (write-u8 2 p)
                    (write-u8 3 p)
                    (bytevector-length (get-output-bytevector p)))"#;
    assert_eq!(eval_to_num(&lisp, &mut eval, expr), 3);
}

#[test]
fn test_get_output_bytevector_contents() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = r#"(let ((p (open-output-bytevector)))
                    (write-u8 65 p)
                    (write-u8 66 p)
                    (let ((bv (get-output-bytevector p)))
                      (bytevector-u8-ref bv 0)))"#;
    assert_eq!(eval_to_num(&lisp, &mut eval, expr), 65);
}

#[test]
fn test_bytevector_port_predicates() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    assert!(eval_is_true(&lisp, &mut eval,
        "(binary-port? (open-input-bytevector #u8(1 2 3)))"));
    assert!(eval_is_true(&lisp, &mut eval,
        "(binary-port? (open-output-bytevector))"));
    assert!(eval_is_true(&lisp, &mut eval,
        "(input-port? (open-input-bytevector #u8(1 2 3)))"));
    assert!(eval_is_true(&lisp, &mut eval,
        "(output-port? (open-output-bytevector))"));
    assert!(eval_is_false(&lisp, &mut eval,
        "(textual-port? (open-input-bytevector #u8(1 2 3)))"));
}

// ============================================================================
// write-string (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_write_string_to_string_port() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = r#"(let ((p (open-output-string)))
                    (write-string "hello" p)
                    (get-output-string p))"#;
    assert_eq!(eval_to_string(&lisp, &mut eval, expr), "\"hello\"");
}

#[test]
fn test_write_string_with_start_end() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = r#"(let ((p (open-output-string)))
                    (write-string "hello" p 1 4)
                    (get-output-string p))"#;
    assert_eq!(eval_to_string(&lisp, &mut eval, expr), "\"ell\"");
}

// ============================================================================
// flush-output-port (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_flush_output_port() {
    use std::fs;

    let path = temp_path("grift_test_flush.txt");
    let _ = fs::remove_file(&path);

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-output-file "{}")))
             (write-string "flushed" p)
             (flush-output-port p)
             (close-port p))"#,
        path
    );
    eval.eval_str(&expr).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "flushed");

    let _ = fs::remove_file(&path);
}

// ============================================================================
// Port predicates for file ports
// ============================================================================

#[test]
fn test_file_port_predicates() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_predicates.txt");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"test").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Textual input file port
    let expr = format!(r#"(let ((p (open-input-file "{}")))
                            (let ((r (list (port? p) (input-port? p) (output-port? p) (textual-port? p) (binary-port? p))))
                              (close-port p)
                              r))"#, path);
    assert_eq!(eval_to_string(&lisp, &mut eval, &expr), "(#t #t #f #t #f)");

    // Binary input file port
    let expr = format!(r#"(let ((p (open-binary-input-file "{}")))
                            (let ((r (list (port? p) (input-port? p) (output-port? p) (textual-port? p) (binary-port? p))))
                              (close-port p)
                              r))"#, path);
    assert_eq!(eval_to_string(&lisp, &mut eval, &expr), "(#t #t #f #f #t)");

    let _ = fs::remove_file(&path);
}

#[test]
fn test_file_port_open_close() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_open_close.txt");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"test").unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-input-file "{}")))
             (let ((before (input-port-open? p)))
               (close-port p)
               (let ((after (input-port-open? p)))
                 (list before after))))"#,
        path
    );
    assert_eq!(eval_to_string(&lisp, &mut eval, &expr), "(#t #f)");

    let _ = fs::remove_file(&path);
}

// ============================================================================
// read-bytevector! (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_read_bytevector_bang() {
    use std::fs;
    use std::io::Write;

    let path = temp_path("grift_test_read_bv_bang.bin");
    {
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(&[10, 20, 30, 40, 50]).unwrap();
    }

    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    let expr = format!(
        r#"(let ((p (open-binary-input-file "{}"))
                 (bv (make-bytevector 5 0)))
             (let ((n (read-bytevector! bv p)))
               (close-port p)
               (list n (bytevector-u8-ref bv 0) (bytevector-u8-ref bv 1))))"#,
        path
    );
    assert_eq!(eval_to_string(&lisp, &mut eval, &expr), "(5 10 20)");

    let _ = fs::remove_file(&path);
}

// ============================================================================
// u8-ready? (R7RS §6.13.2)
// ============================================================================

#[test]
fn test_u8_ready() {
    let lisp: Lisp<20000> = Lisp::new();
    let mut io = grift_std::StdIoProvider::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    eval.set_io_provider(&mut io);

    // Input bytevector ports should always have data ready
    assert!(eval_is_true(&lisp, &mut eval,
        "(u8-ready? (open-input-bytevector #u8(1 2 3)))"));
}
