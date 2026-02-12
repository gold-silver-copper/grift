use grift::{Evaluator, Lisp};
use grift_std::StdIoProvider;
use std::cell::RefCell;

thread_local! {
    static CAPTURED: RefCell<String> = RefCell::new(String::new());
}

fn output_cb<const N: usize>(lisp: &Lisp<N>, val: grift::ArenaIndex) {
    CAPTURED.with(|output| {
        let mut out = output.borrow_mut();
        if val.is_nil() {
            out.push('\n');
        } else if let Ok(grift::Value::String { .. }) = lisp.get(val) {
            let len = lisp.string_len(val).unwrap_or(0);
            for i in 0..len {
                if let Ok(c) = lisp.string_char_at(val, i) {
                    out.push(c);
                }
            }
        } else if let Ok(grift::Value::Char(c)) = lisp.get(val) {
            out.push(c);
        } else {
            out.push_str(&format!("{}", lisp.display(val)));
        }
    });
}

#[test]
fn test_display_to_port() {
    let lisp: Box<Lisp<50000>> = Box::new(Lisp::new());
    let mut eval = Evaluator::new(&*lisp).unwrap();
    let mut io = StdIoProvider::new();
    eval.set_io_provider(&mut io);
    eval.set_output_callback(Some(output_cb::<50000>));

    // Display to stdout (captured) - should NOT have quotes
    CAPTURED.with(|o| o.borrow_mut().clear());
    eval.eval_str(r#"(display "(define x 42)")"#).unwrap();
    let stdout_output = CAPTURED.with(|o| o.borrow().clone());
    println!("Display to stdout: {:?}", stdout_output);
    
    // Write to file via display to port
    eval.eval_str(r#"(call-with-output-file "/tmp/grift-display-port-test.txt"
        (lambda (port) (display "(define x 42)" port)))"#).unwrap();
    let file_content = std::fs::read_to_string("/tmp/grift-display-port-test.txt").unwrap();
    println!("Display to file: {:?}", file_content);
    
    // Check: write to file via write to port 
    eval.eval_str(r#"(call-with-output-file "/tmp/grift-write-port-test.txt"
        (lambda (port) (write "(define x 42)" port)))"#).unwrap();
    let write_content = std::fs::read_to_string("/tmp/grift-write-port-test.txt").unwrap();
    println!("Write to file: {:?}", write_content);
}
