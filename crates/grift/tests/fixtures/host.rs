#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use grift::{Answer, Error, Grift, Value};

/// `(nested thunk)`: calls `thunk` from inside this host call. On the host function's own Rust
/// frame that would let the program choose how deep the Rust stack goes, so every way in is
/// refused, and the program sees the refusal as an error.
fn nested(g: &mut Grift, s: &mut (), args: &[Value]) -> Result<Answer, Error> {
    let top = g.top_level();
    let refused = [
        g.evaluate(s, "1").err(),
        g.evaluate_in(s, &top, "in.grift", "1").err(),
        g.evaluate_library_in(s, &top, "library.grift", "1").err(),
        g.start(s, &args[0], &[]).err(),
        g.start_in(s, &top, "in.grift", "1").err(),
    ];
    for e in refused {
        assert_eq!(g.symbol_name(&e.unwrap().kind).unwrap(), "cannot-nest");
    }
    g.call(s, &args[0], &[]).map(Answer::Now)
}

/// `(on-machine thunk)`: hands the machine the call of `thunk`, which it makes once this host
/// call has returned, on its own frames.
fn on_machine(_: &mut Grift, _: &mut (), args: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Call {
        function: args[0].clone(),
        arguments: Vec::new(),
        then: None,
    })
}

fn main() {
    let mut g: Grift = Grift::new().unwrap();
    for (name, f) in [
        ("nested", nested as grift::HostFunction<()>),
        ("on-machine", on_machine),
    ] {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    // Unoptimized, as embedders develop, on a stack that held 165 levels of nested runs in an
    // optimized build when a host function could call back on its own frame.
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let go = "(define! go (lambda! (call n) (if! (same? n 0) 0 \
                      (add 1 (call (lambda! () (go call (subtract n 1))))))))";
            g.evaluate(&mut (), go).unwrap();
            let e = g.evaluate(&mut (), "(go nested 20000)").unwrap_err();
            assert_eq!(g.symbol_name(&e.kind).unwrap(), "cannot-nest");
            let deep = g.evaluate(&mut (), "(go on-machine 20000)");
            assert_eq!(deep, Ok(Value::Integer(20000)));
        })
        .unwrap()
        .join()
        .unwrap();
}
