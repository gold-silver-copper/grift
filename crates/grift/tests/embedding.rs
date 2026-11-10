//! The embedding's guarantees, from the host's side, through the public API alone:
//! - a run its host function left is over, and a stop reaches only the run its handle was
//!   taken for, never a later one;
//! - a stop ends a walk of a parameter tree, and every host call that is a run.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "a test may abort when memory is refused"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use grift::{Answer, Error, Grift, HostCall, Interrupter, Value};
use std::sync::mpsc;
use std::time::Duration;

/// A handle can be shared between threads, not only sent.
const _: () = {
    const fn shared<T: Send + Sync>() {}
    shared::<Interrupter>()
};

/// What the host functions below work on: what `give` returns and whom `started`
/// tells.
#[derive(Default)]
struct Host {
    give: Option<Result<Value, Error>>,
    started: Option<mpsc::Sender<()>>,
}

type G = Grift<Host>;

fn grift() -> G {
    let mut g = Grift::new().expect("the library loads");
    for (name, f) in [
        ("give", give as grift::HostFunction<Host>),
        ("stop", stop),
        ("stop-and-fail", stop_and_fail),
        ("started", started),
    ] {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    g
}

/// `(give)`: what the host put in its state.
fn give(g: &mut HostCall<Host>, h: &mut Host, _: &[Value]) -> Result<Answer, Error> {
    h.give
        .take()
        .unwrap_or_else(|| Err(g.error("nothing-to-give", &[])))
        .map(Answer::Now)
}

/// `(stop)`: stops the run it is part of, and returns.
fn stop(g: &mut HostCall<Host>, _: &mut Host, _: &[Value]) -> Result<Answer, Error> {
    assert!(g.interrupter().stop(), "a run to stop");
    Ok(Answer::Now(Value::Inert))
}

/// `(started)`: tells the host's other thread that the run is under way.
fn started(_: &mut HostCall<Host>, h: &mut Host, _: &[Value]) -> Result<Answer, Error> {
    h.started.take().unwrap().send(()).unwrap();
    Ok(Answer::Now(Value::Inert))
}

/// `(stop-and-fail)`: stops the run it is part of, and fails.
fn stop_and_fail(g: &mut HostCall<Host>, _: &mut Host, args: &[Value]) -> Result<Answer, Error> {
    g.interrupter().stop();
    let kind = match args.first() {
        Some(Value::Symbol(s)) => g.symbol_name(s)?.to_string(),
        _ => "failed".to_string(),
    };
    Err(g.error(&kind, &[]))
}

fn kind(g: &G, e: &Error) -> String {
    g.symbol_name(&e.kind)
        .unwrap_or("<foreign kind>")
        .to_string()
}

fn outcome(g: &G, r: Result<Value, Error>) -> String {
    match r {
        Ok(v) => {
            let mut s = String::new();
            let _ = g.write(&v, &mut s);
            format!("ok {s}")
        }
        Err(e) => kind(g, &e),
    }
}

/// A host whose function leaves the run it was called from, by a panic.
struct Leaving;

fn panics(_: &mut HostCall<Leaving>, _: &mut Leaving, _: &[Value]) -> Result<Answer, Error> {
    panic!("a host function's bug")
}

fn note(_: &mut HostCall<Leaving>, _: &mut Leaving, _: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Now(Value::Inert))
}

fn leaving() -> Grift<Leaving> {
    let mut g = Grift::new().unwrap();
    for (name, f) in [
        ("panics", panics as grift::HostFunction<Leaving>),
        ("note", note),
        ("listed", |g, _, xs| g.list(xs).map(Answer::Now)),
    ] {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    g
}

#[test]
fn a_run_its_host_function_left_is_over() {
    let kind = |g: &Grift<Leaving>, r: &Result<Value, Error>| match r {
        Ok(_) => "ok".to_string(),
        Err(e) => g.symbol_name(&e.kind).unwrap().to_string(),
    };
    let quiet = |g: &mut Grift<Leaving>, s: &mut Leaving| {
        // A stop with no run in progress does nothing; the next run runs.
        g.interrupter().stop();
        let r = g.evaluate(s, "(add 1 2)");
        assert_eq!(r, Ok(Value::Integer(3)), "{}", kind(g, &r));
    };
    // `(first 1)` built by the host with pair, or with list, is the program's: an error in it
    // names it.
    let program_s = |g: &mut Grift<Leaving>, s: &mut Leaving, by_list: bool, when: &str| {
        let top = g.top_level();
        let name = g.symbol("first").unwrap();
        let first = g.lookup(&top, &name).unwrap().unwrap();
        let form = if by_list {
            g.list(&[first, Value::Integer(1)]).unwrap()
        } else {
            let one = g.pair(Value::Integer(1), Value::EmptyList).unwrap();
            g.pair(first, one).unwrap()
        };
        g.define("form", form).unwrap();
        let e = g
            .evaluate(s, "(evaluate form (current-context))")
            .unwrap_err();
        let mut place = String::new();
        g.write(&e.trace[0], &mut place).unwrap();
        assert_eq!(place, "(#[applicative #[primitive first]] 1)", "{when}");
    };
    let mut s = Leaving;
    // A panic the host catches ends the run.
    let mut g = leaving();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        g.evaluate(&mut s, "(panics)")
    }));
    assert!(caught.is_err());
    quiet(&mut g, &mut s);
    // What a host function builds is the program's, called by library code too.
    let e = g
        .evaluate(
            &mut s,
            "(evaluate (apply listed (list first 1)) (current-context))",
        )
        .unwrap_err();
    let mut place = String::new();
    g.write(&e.trace[0], &mut place).unwrap();
    assert_eq!(place, "(#[applicative #[primitive first]] 1)");
    for by_list in [false, true] {
        // A panic inside library code leaves no mark on what the host builds, before the next
        // run or after it.
        let mut g = leaving();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            g.evaluate(&mut s, "(map panics (list 1))")
        }));
        assert!(caught.is_err());
        program_s(&mut g, &mut s, by_list, "right after the panic");
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            g.evaluate(&mut s, "(map panics (list 1))")
        }));
        assert!(caught.is_err());
        quiet(&mut g, &mut s);
        program_s(&mut g, &mut s, by_list, "after the next run");
    }
    // The arguments call lists are the program's too: here they are evaluated as a combination.
    let mut g = leaving();
    let f = g
        .evaluate(&mut s, "(lambda! args (evaluate args (current-context)))")
        .unwrap();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        g.evaluate(&mut s, "(map panics (list 1))")
    }));
    assert!(caught.is_err());
    let top = g.top_level();
    let name = g.symbol("first").unwrap();
    let first = g.lookup(&top, &name).unwrap().unwrap();
    let e = g.call(&mut s, &f, &[first, Value::Integer(1)]).unwrap_err();
    let mut place = String::new();
    g.write(&e.trace[0], &mut place).unwrap();
    assert_eq!(place, "(#[applicative #[primitive first]] 1)", "called");
}

/// Makes `shared`, a few objects that are a tree of 2^50 leaves, then runs `step` on a thread of
/// its own while stopping it from here: whether the run ended in `interrupted` in time. `step` is
/// a few steps of the machine away after `(started)`, so the wait puts the first stop inside its
/// long walk: a stop that came sooner would end the run before it. The crate's own stop sweep,
/// `a_stop_ends_every_walking_primitive_by_itself`, makes that order certain, one walk at a time.
fn stopped_in_time(step: &'static str) -> bool {
    let (handle, stopper) = mpsc::channel();
    let (tell, told) = mpsc::channel();
    let (finish, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let mut g = grift();
        handle.send(g.interrupter()).unwrap();
        let mut h = Host {
            started: Some(tell),
            ..Host::default()
        };
        let program = format!(
            "(define! grow (lambda! (d n) (if! (same? n 0) d (grow (pair d d) (subtract n 1)))))
             (define! shared (grow () 50))
             (started)
             {step}"
        );
        let r = g.evaluate(&mut h, &program);
        let _ = finish.send(outcome(&g, r));
    });
    let mut stopper: Interrupter = stopper.recv().unwrap();
    told.recv().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    for _ in 0..300 {
        stopper.stop();
        if let Ok(end) = finished.recv_timeout(Duration::from_millis(10)) {
            return end == "interrupted";
        }
    }
    false
}

#[test]
fn a_stop_ends_a_walk_of_a_parameter_tree() {
    assert!(stopped_in_time("(bind (context ()) shared shared)"));
    assert!(stopped_in_time(
        "(evaluate (list vau! shared #ignore 1) (current-context))"
    ));
}

/// A sink that stops its handle's run at the first text it is given.
struct Stopping(Interrupter, Option<bool>, String);

impl std::fmt::Write for Stopping {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.1.get_or_insert_with(|| self.0.stop());
        self.2.push_str(s);
        Ok(())
    }
}

/// Each host call that is a run (G-179's R1 (b)) is work a stop ends. With no run in progress,
/// `write` and `describe` are runs of their own, which their sink stops at its first text, and a
/// stop that ends one is spent with it. Inside a host function, `read`, `read_in` and `bind` join
/// its run, which the function stops first; the reader finds the stop inside one long string,
/// token or comment, before the text's end would fail or end it.
#[test]
fn a_stop_reaches_every_host_run() {
    let mut g: Grift<usize> = Grift::new().unwrap();
    let names = "(define! N (map string-to-symbol (map integer-to-string (range 0 100000)))) N";
    let list = g.evaluate(&mut 0, names).unwrap();
    let e = g.error("example", &vec![Value::Inert; 100_000]);
    for describe in [false, true] {
        let mut sink = Stopping(g.interrupter(), None, String::new());
        let ended = match describe {
            false => g.write(&list, &mut sink),
            true => g.describe(&e, &mut sink, usize::MAX).map(drop),
        };
        let ended = ended.unwrap_err();
        assert_eq!(sink.1, Some(true), "it was the run in progress");
        assert_eq!(g.symbol_name(&ended.kind).unwrap(), "interrupted");
        assert!(sink.2.len() < 100_000, "it stopped part way");
        assert!(!sink.0.stop(), "its run is over");
    }
    let f = g.host_function("joins", |g, run: &mut usize, n| {
        assert!(g.interrupter().stop(), "a run to stop");
        let a = "a".repeat(100_000);
        let joined = match *run {
            0 => g.read(&"1 ".repeat(100_000)),
            1 => g.read(&format!("\"{a}")),
            2 => g.read_in("t", &format!("#{a}")),
            3 => g.read(&format!(";{a}")),
            _ => g
                .context(None)
                .and_then(|c| g.bind(&c, &n[0], n[0]).map(|()| c)),
        };
        assert_eq!(g.symbol_name(&joined.unwrap_err().kind)?, "interrupted");
        Ok(Answer::Now(Value::Inert))
    });
    g.define("joins", f.unwrap()).unwrap();
    for mut run in 0..5 {
        g.evaluate(&mut run, "(joins N)").unwrap_err();
    }
    assert_eq!(g.evaluate(&mut 0, "(add 1 2)"), Ok(Value::Integer(3)));
}
