//! The embedding's guarantees, from the host's side, through the public API alone:
//! - a reclaimed value is `stale-value` and another interpreter's value is `foreign-value`, at
//!   every way in: each primitive's every argument, every reader and writer, every operation on
//!   contexts, and what a host function returns;
//! - a stop reaches only the run in progress when it is asked for, never a later one;
//! - a kept value lives until it is released, and only its own interpreter takes it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use grift::{Error, Grift, Interrupter, Value, WriteError};
use std::sync::mpsc;
use std::time::Duration;

/// What the host functions below work on: what `give` returns, a stopper, and whom `started`
/// tells.
#[derive(Default)]
struct Host {
    give: Option<Result<Value, Error>>,
    stop: Option<Interrupter>,
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
fn give(g: &mut G, h: &mut Host, _: &[Value]) -> Result<Value, Error> {
    h.give
        .take()
        .unwrap_or_else(|| Err(g.error("nothing-to-give", &[])))
}

/// `(stop)`: stops the run it is part of, and returns.
fn stop(_: &mut G, h: &mut Host, _: &[Value]) -> Result<Value, Error> {
    assert!(h.stop.as_mut().unwrap().stop(), "a run to stop");
    Ok(Value::Inert)
}

/// `(started)`: tells the host's other thread that the run is under way.
fn started(_: &mut G, h: &mut Host, _: &[Value]) -> Result<Value, Error> {
    h.started.take().unwrap().send(()).unwrap();
    Ok(Value::Inert)
}

/// `(stop-and-fail)`: stops the run it is part of, and fails.
fn stop_and_fail(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    h.stop.as_mut().unwrap().stop();
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

fn evaluate(g: &mut G, src: &str) -> Value {
    g.evaluate(&mut Host::default(), src).unwrap()
}

/// Runs the machine through several collections.
fn churn(g: &mut G) {
    evaluate(g, "(length (map (lambda! (n) (list n)) (range 0 20000)))");
}

/// One value of every kind that lives in the heap, held by nothing in the interpreter, and a
/// symbol: a pair, two strings, bytes, a context, an operative, an applicative and a host function.
fn objects(g: &mut G) -> Vec<(&'static str, Value)> {
    let unwrapped_host = evaluate(g, "(unwrap give)");
    vec![
        ("pair", g.pair(Value::Integer(1), Value::EmptyList).unwrap()),
        ("string", g.string("one").unwrap()),
        ("other string", g.string("two").unwrap()),
        ("bytes", g.bytes(&[0, 255]).unwrap()),
        ("context", g.context(None).unwrap()),
        ("operative", evaluate(g, "(vau! () #ignore 1)")),
        ("applicative", evaluate(g, "(lambda! () 1)")),
        ("host function", unwrapped_host),
    ]
}

/// Every Rust primitive, as an operative: what the built-in names unwrap to.
fn primitives(g: &mut G) -> Vec<(String, Value)> {
    let names = evaluate(g, "built-in-names");
    let mut found = Vec::new();
    for name in g.list_items(&names).unwrap() {
        let Value::Symbol(s) = &name else { panic!() };
        let text = g.symbol_name(s).unwrap().to_string();
        let v = evaluate(g, &format!("(catch! (unwrap {text}) #ignore #inert)"));
        if matches!(v, Value::Primitive(_)) {
            found.push((text, v));
        }
    }
    found
}

/// Hands `bad` to every way into `g` and lists each that didn't fail with `want`.
fn every_entry_refuses(g: &mut G, what: &str, bad: &Value, want: &str) -> Vec<String> {
    let mut broken = Vec::new();
    let mut check = |entry: String, got: String| {
        if got != want {
            broken.push(format!("{entry} with {what}: {got}"));
        }
    };
    // Each primitive, in each of its first three argument positions.
    for (name, p) in primitives(g) {
        for at in 0..3 {
            let mut arguments = vec![Value::Integer(0); 3];
            arguments[at] = bad.clone();
            let r = g.call(&mut Host::default(), &p, &arguments);
            check(format!("({name} ...) at {at}"), outcome(g, r));
        }
    }
    let top = g.top_level();
    let symbol = g.symbol("x").unwrap();
    let x = Value::Symbol(symbol.clone());
    let r = g.call(&mut Host::default(), bad, &[]);
    check("call as the combiner".into(), outcome(g, r));
    let results: Vec<(&str, Result<(), Error>)> = vec![
        ("define", g.define("x", bad.clone())),
        (
            "pair, first",
            g.pair(bad.clone(), Value::EmptyList).map(drop),
        ),
        (
            "pair, rest",
            g.pair(Value::EmptyList, bad.clone()).map(drop),
        ),
        ("list", g.list(std::slice::from_ref(bad)).map(drop)),
        ("bind, the value", g.bind(&top, &x, bad.clone())),
        ("bind, the context", g.bind(bad, &x, Value::Inert)),
        ("bind, the parameters", g.bind(&top, bad, Value::Inert)),
        // Before the context's kind is checked, as a primitive's arguments are.
        (
            "bind, the value, with no context",
            g.bind(&Value::Integer(1), &x, bad.clone()),
        ),
        (
            "bind, the parameters, with no context",
            g.bind(&Value::Integer(1), bad, Value::Inert),
        ),
        ("lookup", g.lookup(bad, &symbol).map(drop)),
        ("position", g.position(bad).map(drop)),
        ("names", g.names(bad).map(drop)),
        ("context", g.context(Some(bad)).map(drop)),
        ("keep", g.keep(bad.clone()).map(drop)),
        ("as_pair", g.as_pair(bad).map(drop)),
        ("as_str", g.as_str(bad).map(drop)),
        ("as_bytes", g.as_bytes(bad).map(drop)),
        ("list_items", g.list_items(bad).map(drop)),
    ];
    for (entry, r) in results {
        check(entry.into(), r.err().map_or("ok".into(), |e| kind(g, &e)));
    }
    let r = g.evaluate_in(&mut Host::default(), bad, "x.grift", "1");
    check("evaluate_in, the context".into(), outcome(g, r));
    // Written, and in an error described.
    let written = g.write(bad, &mut String::new());
    let e = g.error("mine", std::slice::from_ref(bad));
    let described = g.describe(&e, &mut String::new());
    for (entry, r) in [("write", written), ("describe", described)] {
        let got = match r {
            Err(WriteError::Value(e)) => kind(g, &e),
            r => format!("{r:?}"),
        };
        check(entry.into(), got);
    }
    // From a host function, as its value and in its error.
    let mut h = Host {
        give: Some(Ok(bad.clone())),
        ..Host::default()
    };
    let r = g.evaluate(&mut h, "(give)");
    check("a host function's value".into(), outcome(g, r));
    let mut h = Host {
        give: Some(Err(g.error("mine", std::slice::from_ref(bad)))),
        ..Host::default()
    };
    // An error the program can catch, as any host function's: it sees the kind.
    let r = g.evaluate(&mut h, "(catch! (give) e (evaluate-in! kind e))");
    let got = outcome(g, r).replace("ok ", "");
    check("a host function's error".into(), got);
    broken
}

#[test]
fn a_reclaimed_value_is_stale_value_everywhere() {
    let mut g = grift();
    let old = objects(&mut g);
    churn(&mut g);
    let mut broken = Vec::new();
    for (what, v) in &old {
        // The host function is no heap object: it can't go stale.
        if !matches!(v, Value::Host(_)) {
            broken.extend(every_entry_refuses(&mut g, what, v, "stale-value"));
        }
    }
    // Two reclaimed strings are neither the same nor different: same? refuses them at the door.
    let same = evaluate(&mut g, "same?");
    let r = g.call(
        &mut Host::default(),
        &same,
        &[old[1].1.clone(), old[2].1.clone()],
    );
    if outcome(&g, r) != "stale-value" {
        broken.push("same? of two reclaimed strings".into());
    }
    assert!(
        broken.is_empty(),
        "{} broken:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

#[test]
fn another_interpreters_value_is_foreign_value_everywhere() {
    // Two interpreters built the same way: values made by the same steps have the same slots and
    // ids, so only a check can tell them apart.
    let (mut theirs, mut ours) = (grift(), grift());
    let foreign = objects(&mut theirs);
    let _ours = objects(&mut ours);
    let mut broken = Vec::new();
    for (what, v) in &foreign {
        broken.extend(every_entry_refuses(&mut ours, what, v, "foreign-value"));
    }
    let symbol = Value::Symbol(theirs.symbol("only-theirs").unwrap());
    ours.symbol("only-ours").unwrap();
    broken.extend(every_entry_refuses(
        &mut ours,
        "a symbol",
        &symbol,
        "foreign-value",
    ));
    // An error of theirs: its kind is their symbol.
    let e = theirs.error("only-theirs", &[]);
    if ours.symbol_name(&e.kind).is_ok() {
        broken.push("the kind of their error".into());
    }
    let Value::Symbol(s) = &symbol else { panic!() };
    if ours.symbol_name(s).is_ok() {
        broken.push("symbol_name of their symbol".into());
    }
    match ours.lookup(&Value::Integer(1), s) {
        Err(e) if kind(&ours, &e) == "foreign-value" => {}
        r => broken.push(format!("lookup of their symbol, with no context: {r:?}")),
    }
    // A host function of theirs never runs ours, whatever its number.
    let host = evaluate(&mut theirs, "(unwrap stop)");
    let mut h = Host {
        stop: Some(ours.interrupter()),
        ..Host::default()
    };
    let r = ours.call(&mut h, &host, &[]);
    if outcome(&ours, r) != "foreign-value" {
        broken.push("calling their host function".into());
    }
    // Values of an interpreter that is gone stay foreign: its identity can't be reused.
    let gone = {
        let mut g = grift();
        g.string("gone").unwrap()
    };
    let fresh = grift();
    if fresh.as_str(&gone).is_ok() {
        broken.push("a dropped interpreter's string".into());
    }
    assert!(
        broken.is_empty(),
        "{} broken:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

#[test]
fn a_stop_reaches_only_the_run_in_progress() {
    let mut g = grift();
    let mut h = Host {
        stop: Some(g.interrupter()),
        ..Host::default()
    };
    let mut broken = Vec::new();
    let mut expect = |g: &mut G, h: &mut Host, what: &str, src: &str, want: &str| {
        let r = g.evaluate(h, src);
        let got = outcome(g, r);
        if got != want {
            broken.push(format!("{what}: {src} gave {got}, not {want}"));
        }
    };
    // A stop with no run in progress does nothing: before any run, and between two.
    assert!(!h.stop.as_mut().unwrap().stop(), "no run to stop");
    expect(&mut g, &mut h, "a stop before any run", "(add 1 2)", "ok 3");
    assert!(!h.stop.as_mut().unwrap().stop(), "no run to stop");
    expect(&mut g, &mut h, "a stop between runs", "(add 1 2)", "ok 3");
    // A stop in the body stops its run, and here the cleanups run. A later stop stops it again, so
    // a cleanup that never ends can still be ended.
    expect(
        &mut g,
        &mut h,
        "a stop in a run",
        "(define! log ()) (define! here (current-context))
         (finally! (sequence! (stop) (add 1 2)) (bind here (quote! log) (pair 1 log)))",
        "interrupted",
    );
    expect(&mut g, &mut h, "the cleanup ran", "log", "ok (1)");
    expect(
        &mut g,
        &mut h,
        "a second stop, in a cleanup that doesn't end",
        "(define! spin (lambda! () (spin))) (finally! (stop) (stop) (spin))",
        "interrupted",
    );
    // A stop in the run's last host call, after which it takes no step, still stops it, and never
    // reaches the next run.
    for f in ["stop", "stop-and-fail"] {
        let f = evaluate(&mut g, f);
        let r = g.call(&mut h, &f, &[]);
        assert_eq!(
            outcome(&g, r),
            "interrupted",
            "a stop in the last host call"
        );
        expect(&mut g, &mut h, "a stop as a run ended", "(add 1 2)", "ok 3");
    }
    // An evaluation inside a host function is part of the run that called it.
    let f = g.host_function("nested", nested).unwrap();
    g.define("nested", f).unwrap();
    expect(
        &mut g,
        &mut h,
        "a stop in a nested run",
        "(catch! (nested) #ignore 0)",
        "interrupted",
    );
    expect(&mut g, &mut h, "the run after it", "(add 1 2)", "ok 3");
    // A host function that stops its run and then fails, even with a limit, fails it with that
    // stop: here the cleanups run, once.
    for failure in ["failed", "interrupted", "out-of-memory"] {
        let src = format!(
            "(define! cleaned 0)
             (catch! (finally! (stop-and-fail (quote! {failure})) (bind here (quote! cleaned) (add cleaned 1))) e 42)"
        );
        expect(
            &mut g,
            &mut h,
            "a stop, then a failure",
            &src,
            "interrupted",
        );
        expect(&mut g, &mut h, "the cleanup ran once", "cleaned", "ok 1");
    }
    // A host function that handles its nested run's interruption itself lets the run go on, and
    // the next stop stops it.
    let f = g.host_function("swallow", swallow).unwrap();
    g.define("swallow", f).unwrap();
    expect(
        &mut g,
        &mut h,
        "a stop after a swallowed one",
        "(swallow) (stop) (spin)",
        "interrupted",
    );
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

/// `(swallow)`: a nested run that is stopped, its `interrupted` handled here.
fn swallow(g: &mut G, h: &mut Host, _: &[Value]) -> Result<Value, Error> {
    let _ = g.evaluate(h, "(stop) (add 1 2)");
    Ok(Value::Inert)
}

/// A host whose functions leave the run they were called from: by a panic, by replacing the
/// interpreter, or by putting it back after a run inside was given up.
#[derive(Default)]
struct Leaving {
    saved: Option<Grift<Leaving>>,
}

fn panics(_: &mut Grift<Leaving>, _: &mut Leaving, _: &[Value]) -> Result<Value, Error> {
    panic!("a host function's bug")
}

fn swap_out(g: &mut Grift<Leaving>, s: &mut Leaving, _: &[Value]) -> Result<Value, Error> {
    s.saved = Some(std::mem::replace(g, leaving()));
    Ok(Value::Integer(0))
}

fn restore(g: &mut Grift<Leaving>, s: &mut Leaving, _: &[Value]) -> Result<Value, Error> {
    let _ = g.evaluate(s, "(finally! (swap-out) (note))");
    if let Some(old) = s.saved.take() {
        *g = old;
    }
    Err(g.error("restored", &[]))
}

fn note(_: &mut Grift<Leaving>, _: &mut Leaving, _: &[Value]) -> Result<Value, Error> {
    Ok(Value::Inert)
}

fn leaving() -> Grift<Leaving> {
    let mut g = Grift::new().unwrap();
    for (name, f) in [
        ("panics", panics as grift::HostFunction<Leaving>),
        ("swap-out", swap_out),
        ("restore", restore),
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
    let mut s = Leaving::default();
    // A panic the host catches ends the run.
    let mut g = leaving();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        g.evaluate(&mut s, "(panics)")
    }));
    assert!(caught.is_err());
    quiet(&mut g, &mut s);
    // Replaced: the run ends with foreign-value, and the rest of the program doesn't run.
    let mut g = leaving();
    let r = g.evaluate(&mut s, "(swap-out) (quote! rest)");
    assert_eq!(kind(&g, &r), "foreign-value");
    assert!(r.unwrap_err().trace.is_empty());
    quiet(&mut g, &mut s);
    // The one swapped out, put back: its run is over, and it runs the next.
    let mut parked = s.saved.take().unwrap();
    quiet(&mut parked, &mut s);
    // Put back after a run inside it was given up: neither that run's cleanup nor its frames
    // reach the run around it.
    let mut g = leaving();
    let r = g.evaluate(&mut s, "(restore)");
    assert_eq!(kind(&g, &r), "foreign-value");
    assert!(r.unwrap_err().trace.is_empty());
    quiet(&mut g, &mut s);
}

/// `(nested)`: evaluates `(stop)` from inside the host.
fn nested(g: &mut G, h: &mut Host, _: &[Value]) -> Result<Value, Error> {
    g.evaluate(h, "(stop) (add 1 2)")
}

#[test]
fn a_timer_thread_sets_a_time_limit() {
    // Time limits are the host's: a thread that stops the run after a while. The run tells it
    // when it is under way, so the stop can't come before it.
    let mut g = grift();
    let (tell, told) = mpsc::channel();
    let mut timer = g.interrupter();
    let thread = std::thread::spawn(move || {
        told.recv().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        timer.stop();
    });
    let mut h = Host {
        started: Some(tell),
        ..Host::default()
    };
    let r = g.evaluate(
        &mut h,
        "(define! spin (lambda! (n) (spin (add n 1)))) (started) (spin 0)",
    );
    thread.join().unwrap();
    assert_eq!(outcome(&g, r), "interrupted");
    let r = g.evaluate(&mut h, "(add 1 2)");
    assert_eq!(outcome(&g, r), "ok 3");
    // A stop that finds the run while its last step, a long native call, is under way stops it.
    // The thread's first stop finds the run as the call begins, far from the moment after the
    // run's last check, where a stop still says `true` and the run returns its value.
    let parse = evaluate(&mut g, "string-to-integer");
    let digits = g.string(&"0".repeat(16 << 20)).unwrap();
    let (go, gone) = mpsc::channel();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (mut timer, finished) = (g.interrupter(), std::sync::Arc::clone(&done));
    let thread = std::thread::spawn(move || {
        gone.recv().unwrap();
        while !finished.load(std::sync::atomic::Ordering::SeqCst) {
            if timer.stop() {
                return true;
            }
        }
        false
    });
    go.send(()).unwrap();
    let r = g.call(&mut h, &parse, &[digits]);
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    if thread.join().unwrap() {
        assert_eq!(
            outcome(&g, r),
            "interrupted",
            "a stop found the run, and was lost"
        );
    }
    let r = g.evaluate(&mut h, "(add 1 2)");
    assert_eq!(outcome(&g, r), "ok 3");
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

#[test]
fn a_stop_ends_printing_a_shared_tree() {
    assert!(stopped_in_time("(value-to-string shared)"));
}

#[test]
fn a_kept_value_lives_until_it_is_released() {
    let mut g = grift();
    let handler = evaluate(&mut g, "(lambda! (x) (list x x))");
    let kept = g.keep(handler).unwrap();
    churn(&mut g);
    let handler = g.kept(&kept).unwrap();
    let r = g.call(&mut Host::default(), &handler, &[Value::Integer(7)]);
    assert_eq!(outcome(&g, r), "ok (7 7)");
    // Released, nothing holds it any more.
    g.release(kept).unwrap();
    churn(&mut g);
    assert_eq!(kind(&g, &g.as_pair(&handler).unwrap_err()), "stale-value");
    // Keeping and releasing on every event never runs out, and no program sees the names.
    for n in 0..100_000 {
        let k = g.keep(Value::Integer(n)).unwrap();
        assert_eq!(g.kept(&k).unwrap(), Value::Integer(n));
        g.release(k).unwrap();
    }
    // Another interpreter's Kept is refused and handed back, to be released where it belongs.
    let mut other = grift();
    let theirs = other.keep(Value::Integer(1)).unwrap();
    assert_eq!(kind(&g, &g.kept(&theirs).unwrap_err()), "foreign-value");
    let (e, theirs) = g.release(theirs).unwrap_err();
    assert_eq!(kind(&g, &e), "foreign-value");
    other.release(theirs).unwrap();
}

/// `v`'s reference in every other heap variant.
fn retagged(v: &Value) -> Vec<Value> {
    let r = match v {
        Value::Pair(r)
        | Value::String(r)
        | Value::Bytes(r)
        | Value::Context(r)
        | Value::Operative(r)
        | Value::Applicative(r) => r.clone(),
        _ => return Vec::new(),
    };
    let all = [
        Value::Pair(r.clone()),
        Value::String(r.clone()),
        Value::Bytes(r.clone()),
        Value::Context(r.clone()),
        Value::Operative(r.clone()),
        Value::Applicative(r),
    ];
    all.into_iter()
        .filter(|w| std::mem::discriminant(w) != std::mem::discriminant(v))
        .collect()
}

#[test]
fn a_reference_in_another_variant_is_stale_value_everywhere() {
    // Safe Rust can put a live object's reference in any variant: it is no value of the
    // interpreter's, and never read as an object of another kind.
    let mut g = grift();
    let live = objects(&mut g);
    let mut broken = Vec::new();
    for (what, v) in &live {
        for w in retagged(v) {
            let what = format!(
                "{what} as {}",
                format!("{w:?}").split('(').next().unwrap_or("")
            );
            broken.extend(every_entry_refuses(&mut g, &what, &w, "stale-value"));
        }
    }
    assert!(
        broken.is_empty(),
        "{} broken:\n{}",
        broken.len(),
        broken.join("\n")
    );
}

#[test]
fn bytes_copy_the_input_and_borrow_the_payload() {
    let mut g = grift();
    let all: Vec<u8> = (0..=255).collect();
    let mut input = all.clone();
    let v = g.bytes(&input).unwrap();
    input.fill(0);
    assert_eq!(g.as_bytes(&v).unwrap(), Some(all.as_slice()));
    assert_eq!(g.as_str(&v).unwrap(), None);
    let kept = g.keep(v).unwrap();
    churn(&mut g);
    assert_eq!(
        g.as_bytes(&g.kept(&kept).unwrap()).unwrap(),
        Some(all.as_slice())
    );
    g.release(kept).unwrap();
    let empty = g.bytes(&[]).unwrap();
    assert_eq!(g.as_bytes(&empty).unwrap(), Some(&[][..]));
    assert_eq!(g.as_bytes(&Value::Integer(0)).unwrap(), None);
    let text = g.string("abc").unwrap();
    assert_eq!(g.as_bytes(&text).unwrap(), None);
}
