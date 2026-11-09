//! The embedding's guarantees, from the host's side, through the public API alone:
//! - a reclaimed value is `stale-value` and another interpreter's value is `foreign-value`, at
//!   every way in: each primitive's every argument, every reader and writer, every operation on
//!   contexts, and what a host function returns;
//! - a stop reaches only the run its handle was taken for, never a later one;
//! - a kept value lives until it is released, and only its own interpreter takes it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use grift::{Answer, Error, Grift, Interrupter, Value};
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
fn give(g: &mut G, h: &mut Host, _: &[Value]) -> Result<Answer, Error> {
    h.give
        .take()
        .unwrap_or_else(|| Err(g.error("nothing-to-give", &[])))
        .map(Answer::Now)
}

/// `(stop)`: stops the run it is part of, and returns.
fn stop(g: &mut G, _: &mut Host, _: &[Value]) -> Result<Answer, Error> {
    assert!(g.interrupter().stop(), "a run to stop");
    Ok(Answer::Now(Value::Inert))
}

/// `(started)`: tells the host's other thread that the run is under way.
fn started(_: &mut G, h: &mut Host, _: &[Value]) -> Result<Answer, Error> {
    h.started.take().unwrap().send(()).unwrap();
    Ok(Answer::Now(Value::Inert))
}

/// `(stop-and-fail)`: stops the run it is part of, and fails.
fn stop_and_fail(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Answer, Error> {
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

/// A host whose functions leave the run they were called from: by a panic, or by replacing the
/// interpreter.
#[derive(Default)]
struct Leaving {
    saved: Option<Grift<Leaving>>,
}

fn panics(_: &mut Grift<Leaving>, _: &mut Leaving, _: &[Value]) -> Result<Answer, Error> {
    panic!("a host function's bug")
}

fn swap_out(g: &mut Grift<Leaving>, s: &mut Leaving, _: &[Value]) -> Result<Answer, Error> {
    s.saved = Some(std::mem::replace(g, leaving()));
    Ok(Answer::Now(Value::Integer(0)))
}

fn note(_: &mut Grift<Leaving>, _: &mut Leaving, _: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Now(Value::Inert))
}

fn leaving() -> Grift<Leaving> {
    let mut g = Grift::new().unwrap();
    for (name, f) in [
        ("panics", panics as grift::HostFunction<Leaving>),
        ("swap-out", swap_out),
        ("note", note),
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
    let mut s = Leaving::default();
    // A panic the host catches ends the run.
    let mut g = leaving();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        g.evaluate(&mut s, "(panics)")
    }));
    assert!(caught.is_err());
    quiet(&mut g, &mut s);
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
        // Nor does library code that swapped the interpreter out, on the one swapped out.
        let mut g = leaving();
        let r = g.evaluate(&mut s, "(map swap-out (list 1))");
        assert_eq!(kind(&g, &r), "foreign-value");
        let mut parked = s.saved.take().unwrap();
        program_s(&mut parked, &mut s, by_list, "swapped out by library code");
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
    // Replaced: the run ends with foreign-value, and the rest of the program doesn't run.
    let mut g = leaving();
    let r = g.evaluate(&mut s, "(swap-out) (quote! rest)");
    assert_eq!(kind(&g, &r), "foreign-value");
    assert!(r.unwrap_err().trace.is_empty());
    quiet(&mut g, &mut s);
    // The one swapped out, put back: its run is over, and it runs the next.
    let mut parked = s.saved.take().unwrap();
    quiet(&mut parked, &mut s);
}

/// Makes `shared`, a few objects that are a tree of 2^50 leaves, then runs `step` on a thread of
/// its own while stopping it from here: whether the run ended in `interrupted` in time. `step` is
/// a few steps of the machine away after `(started)`, so the wait puts the first stop inside its
/// long walk: a stop that came sooner would end the run before it. The crate's own test
/// `each_long_walk_finds_a_stop_by_itself` makes that order certain, one walk at a time.
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
