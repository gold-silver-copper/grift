#![allow(clippy::unwrap_used, clippy::panic)]

use grift::{Error, Grift, Value};

fn source(g: &mut Grift<usize>, depth: &mut usize, _: &[Value]) -> Result<Value, Error> {
    let context = g.top_level();
    if *depth > 0 {
        *depth -= 1;
        let result = g.evaluate_in(depth, &context, "wrapper.grift", "(source)");
        *depth += 1;
        result
    } else {
        let read = g.evaluate(depth, "read-form")?;
        let text = g.string("(add 1 2)")?;
        g.call(depth, &read, &[text, Value::Integer(0)])?;
        g.evaluate_in(depth, &context, "source.grift", "(add 1 2)")
    }
}

fn main() {
    let mut g = Grift::new().unwrap();
    let host = g.host_function("source", source).unwrap();
    g.define("source", host).unwrap();
    // At least 8 KiB above the passing 292 KiB control; d9dc54a fails at 300 KiB.
    let mut depth = 16;
    std::thread::Builder::new()
        .stack_size(300 * 1024)
        .spawn(move || {
            let context = g.top_level();
            // Fixed host nesting, independent of how many sources the grift loop processes.
            assert_eq!(
                g.evaluate_in(
                    &mut depth,
                    &context,
                    "driver.grift",
                    "
                    (define! loop (lambda! (n) (if! (same? n 0) #true
                        (sequence! (source) (loop (subtract n 1))))))
                    (loop 1000)"
                ),
                Ok(Value::Boolean(true)),
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
