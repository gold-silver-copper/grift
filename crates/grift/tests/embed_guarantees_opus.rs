//! The embedding's guarantees, checked from the host's side through the public API:
//! - a reclaimed value is always `stale-value`, never another answer;
//! - a value or symbol from another interpreter is detected, never read as this one's;
//! - a stop reaches only the run in progress when it is asked for, never a later one.
//!
//! Each test gathers every way it finds the guarantee broken and reports them all at once.
//! Where today's API can't return the answer the guarantee asks for (`as_pair` and `as_str` give
//! an `Option`, the interrupt is a `&'static AtomicBool`), the adaptation is in one helper below,
//! marked "Adapt".

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use grift::{Error, Grift, Value};
use std::sync::atomic::{AtomicBool, Ordering};

fn grift<T>() -> Grift<T> {
    Grift::new().expect("the library loads")
}

fn show<T>(g: &Grift<T>, v: Value) -> String {
    let mut s = String::new();
    g.write(v, &mut s).unwrap();
    s
}

fn kind<T>(g: &Grift<T>, e: &Error) -> String {
    show(g, Value::Symbol(e.kind))
}

/// What a result was, as text: `ok <value>` or `error <kind>`.
fn outcome<T>(g: &Grift<T>, r: &Result<Value, Error>) -> String {
    match r {
        Ok(v) => format!("ok {}", show(g, *v)),
        Err(e) => format!("error {}", kind(g, e)),
    }
}

/// Fails listing every broken case, if there is one.
fn report(guarantee: &str, broken: &[String]) {
    assert!(
        broken.is_empty(),
        "{guarantee}; broken {} time(s):\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// What a Rust-side reader made of a value.
#[derive(Debug, PartialEq)]
enum Read {
    /// It answered, as if the value were this interpreter's live object.
    Answer(String),
    /// It said "not that type".
    NotThatType,
    /// It failed with an error of this kind.
    Error(String),
}

/// Adapt: `as_pair` today answers `Option`; once it can fail, map its error to `Read::Error`.
fn read_pair<T>(g: &Grift<T>, v: Value) -> Read {
    match g.as_pair(v) {
        Some((a, b)) => Read::Answer(format!("({} . {})", show(g, a), show(g, b))),
        None => Read::NotThatType,
    }
}

/// Adapt: `as_str` today answers `Option`; once it can fail, map its error to `Read::Error`.
fn read_str<T>(g: &Grift<T>, v: Value) -> Read {
    match g.as_str(v) {
        Some(s) => Read::Answer(format!("{s:?}")),
        None => Read::NotThatType,
    }
}

/// The value bound to `name`, a primitive or library combiner.
fn combiner<T>(g: &mut Grift<T>, state: &mut T, name: &str) -> Value {
    g.evaluate(state, name).unwrap()
}

/// Runs the machine long enough for several collections, so whatever the host holds unbound is
/// reclaimed.
fn churn<T>(g: &mut Grift<T>, state: &mut T) {
    g.evaluate(
        state,
        "(length (map (lambda! (n) (list n)) (range 0 20000)))",
    )
    .unwrap();
}

/// Checks the test's premise: `v` really was reclaimed. `first` on a pair and `read-form` on a
/// string already say `stale-value` for a reclaimed one, so they tell.
fn assert_reclaimed<T>(g: &mut Grift<T>, state: &mut T, v: Value) {
    let (probe, arguments) = match v {
        Value::Pair(_) => ("first", vec![v]),
        Value::String(_) => ("read-form", vec![v, Value::Integer(0)]),
        Value::Context(_) => ("names", vec![v]),
        _ => panic!("no probe for {v:?}"),
    };
    let p = combiner(g, state, probe);
    let r = g.call(state, p, &arguments);
    assert_eq!(
        outcome(g, &r),
        "error stale-value",
        "premise: {v:?} was reclaimed"
    );
}

// Guarantee 4: a reclaimed value is always `stale-value`.

#[test]
fn rust_side_readers_say_stale_value_for_a_reclaimed_value() {
    let mut g: Grift = grift();
    let pair = g.pair(Value::Integer(1), Value::EmptyList).unwrap();
    let list = g.pair(Value::Integer(0), pair).unwrap();
    let string = g.string("gone").unwrap();
    churn(&mut g, &mut ());
    for v in [list, string] {
        assert_reclaimed(&mut g, &mut (), v);
    }
    let stale = Read::Error("stale-value".into());
    let mut broken = Vec::new();
    // A list walk that reads "not a pair" for a reclaimed list takes it for a shorter list.
    let got = read_pair(&g, list);
    if got != stale {
        broken.push(format!("as_pair of a reclaimed pair answered {got:?}"));
    }
    let got = read_str(&g, string);
    if got != stale {
        broken.push(format!("as_str of a reclaimed string answered {got:?}"));
    }
    report("a reclaimed value read from Rust is stale-value", &broken);
}

#[test]
fn primitives_argument_checks_say_stale_value_for_a_reclaimed_value() {
    let mut g: Grift = grift();
    let string = g.string("gone").unwrap();
    let list = g.pair(Value::Integer(1), Value::EmptyList).unwrap();
    // An error record, kept bound, so only its `values` list will be reclaimed.
    g.evaluate(&mut (), "(define! record (error (quote! oops) 1))")
        .unwrap();
    churn(&mut g, &mut ());
    assert_reclaimed(&mut g, &mut (), string);
    assert_reclaimed(&mut g, &mut (), list);
    let record = combiner(&mut g, &mut (), "record");
    let values = Value::Symbol(g.symbol("values").unwrap());
    let bind = combiner(&mut g, &mut (), "bind");
    g.call(&mut (), bind, &[record, values, list]).unwrap();

    let (zero, one) = (Value::Integer(0), Value::Integer(1));
    let cases: &[(&str, Vec<Value>)] = &[
        ("string-byte-length", vec![string]),
        ("string-byte", vec![string, zero]),
        ("byte-substring", vec![string, zero, one]),
        ("string-append", vec![string]),
        ("string-to-symbol", vec![string]),
        ("string-to-integer", vec![string]),
        ("string-to-float", vec![string]),
        // `raise` reads the record's `values`, now a reclaimed list.
        ("raise", vec![record]),
    ];
    let mut broken = Vec::new();
    for (name, arguments) in cases {
        let p = combiner(&mut g, &mut (), name);
        let r = g.call(&mut (), p, arguments);
        let got = outcome(&g, &r);
        if got != "error stale-value" {
            broken.push(format!("({name} <reclaimed> ...) gave {got}"));
        }
    }
    report(
        "a primitive given a reclaimed value fails with stale-value",
        &broken,
    );
}

#[test]
fn same_never_compares_reclaimed_strings() {
    let mut g: Grift = grift();
    let a = g.string("one").unwrap();
    let b = g.string("two").unwrap();
    churn(&mut g, &mut ());
    assert_reclaimed(&mut g, &mut (), a);
    assert_reclaimed(&mut g, &mut (), b);
    let live = g.string("one").unwrap();
    let same = combiner(&mut g, &mut (), "same?");
    let mut broken = Vec::new();
    // Two different strings, both gone, are not "the same", and a gone one is not unlike a live
    // one: either answer is made up.
    for (what, x, y) in [
        ("two reclaimed strings", a, b),
        ("a reclaimed string and a live one", a, live),
    ] {
        let r = g.call(&mut (), same, &[x, y]);
        let got = outcome(&g, &r);
        if got != "error stale-value" {
            broken.push(format!("same? of {what} gave {got}"));
        }
    }
    report(
        "same? on a reclaimed string fails with stale-value",
        &broken,
    );
}

// Guarantee 5: a value or symbol from another interpreter is detected.

fn answer_one(_: &mut Grift, _: &mut (), _: &[Value]) -> Result<Value, Error> {
    Ok(Value::Integer(1))
}

fn answer_two(_: &mut Grift, _: &mut (), _: &[Value]) -> Result<Value, Error> {
    Ok(Value::Integer(2))
}

#[test]
fn another_interpreters_value_is_detected() {
    let (mut first, mut second): (Grift, Grift) = (grift(), grift());
    // Two fresh interpreters are built the same way, so values made by the same steps in each
    // have the same bits: only a check can tell them apart.
    let theirs_string = first.string("one").unwrap();
    second.string("three").unwrap();
    let theirs_pair = first.pair(Value::Integer(1), Value::EmptyList).unwrap();
    second.pair(Value::Integer(3), Value::EmptyList).unwrap();
    let theirs_function = first.host_function("one", answer_one).unwrap();
    second.host_function("two", answer_two).unwrap();

    let mut broken = Vec::new();
    // Read from Rust.
    for (what, got) in [
        ("as_str", read_str(&second, theirs_string)),
        ("as_pair", read_pair(&second, theirs_pair)),
    ] {
        if !matches!(got, Read::Error(_)) {
            broken.push(format!("{what} of the other's value answered {got:?}"));
        }
    }
    // Handed to the machine: as a call's combiner, as its argument, as a definition.
    let length = combiner(&mut second, &mut (), "string-byte-length");
    let r = second.call(&mut (), theirs_function, &[]);
    if r.is_ok() {
        let got = outcome(&second, &r);
        broken.push(format!("calling the other's host function gave {got}"));
    }
    let r = second.call(&mut (), length, &[theirs_string]);
    if r.is_ok() {
        let got = outcome(&second, &r);
        broken.push(format!(
            "(string-byte-length <the other's \"one\">) gave {got}"
        ));
    }
    let defined = second.define("theirs", theirs_pair);
    let r = defined.and_then(|()| second.evaluate(&mut (), "(first theirs)"));
    if r.is_ok() {
        let got = outcome(&second, &r);
        broken.push(format!("(first <the other's (1)>), defined, gave {got}"));
    }
    report("another interpreter's value is an error", &broken);
}

/// A host function that fails with the error the host keeps in its state.
fn fail(_: &mut Grift<Option<Error>>, e: &mut Option<Error>, _: &[Value]) -> Result<Value, Error> {
    Err(e.take().unwrap())
}

#[test]
fn another_interpreters_symbol_is_detected() {
    let mut first: Grift<Option<Error>> = grift();
    let mut second: Grift<Option<Error>> = grift();
    // The next name each interns gets the same id.
    let theirs = first.symbol("only-in-the-first").unwrap();
    second.symbol("only-in-the-second").unwrap();

    let mut broken = Vec::new();
    // As an argument.
    let print = combiner(&mut second, &mut None, "value-to-string");
    let r = second.call(&mut None, print, &[Value::Symbol(theirs)]);
    if r.is_ok() {
        let got = outcome(&second, &r);
        broken.push(format!("(value-to-string <the other's symbol>) gave {got}"));
    }
    // As the kind of an error a host function passes on from the other interpreter, as a host
    // that runs two interpreters would with `?`. Nobody raised `only-in-the-second`.
    let mut state = Some(first.error("only-in-the-first", &[]));
    let f = second.host_function("fail", fail).unwrap();
    second.define("fail", f).unwrap();
    let r = second.evaluate(&mut state, "(catch! (fail) e (evaluate-in! kind e))");
    let got = outcome(&second, &r);
    if got.contains("only-in-the-second") {
        broken.push(format!("catching the other's error kind gave {got}"));
    }
    report("another interpreter's symbol is an error", &broken);
}

// Ruling 2: a stop reaches only the run in progress when it is asked for.

/// Adapt: how the host asks for a stop. Today it owns a flag it sets; with the new API this holds
/// the interpreter's `Interrupter`, and `stopper` asks the interpreter for one.
struct Stop(&'static AtomicBool);

impl Stop {
    fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn stopper<T>(g: &mut Grift<T>) -> Stop {
    let flag = Box::leak(Box::new(AtomicBool::new(false)));
    g.set_interrupt_flag(flag);
    Stop(flag)
}

/// A host function that stops the run it is part of, then returns.
fn stop_now(_: &mut Grift<Stop>, s: &mut Stop, _: &[Value]) -> Result<Value, Error> {
    s.stop();
    Ok(Value::Inert)
}

/// A host function that stops the run it is part of, then fails.
fn stop_and_fail(g: &mut Grift<Stop>, s: &mut Stop, _: &[Value]) -> Result<Value, Error> {
    s.stop();
    Err(g.error("failed", &[]))
}

#[test]
fn a_stop_with_no_run_in_progress_does_nothing() {
    let mut g: Grift<Stop> = grift();
    let mut s = stopper(&mut g);
    let mut broken = Vec::new();
    // Before any run, and between two runs.
    for when in ["before the first run", "after a run"] {
        s.stop();
        let r = g.evaluate(&mut s, "(add 1 2)");
        let got = outcome(&g, &r);
        if got != "ok 3" {
            broken.push(format!("a run after a stop asked for {when} gave {got}"));
        }
    }
    report("a stop with no run in progress does nothing", &broken);
}

#[test]
fn a_stop_during_a_run_never_reaches_the_next() {
    let mut g: Grift<Stop> = grift();
    let mut s = stopper(&mut g);
    for (name, f) in [
        ("stop-now", stop_now as grift::HostFunction<Stop>),
        ("stop-and-fail", stop_and_fail),
    ] {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    // The premise: a stop does stop the run it is asked for in.
    let r = g.evaluate(&mut s, "(sequence! (stop-now) (add 1 2))");
    assert_eq!(
        outcome(&g, &r),
        "error interrupted",
        "premise: a stop stops its run"
    );

    let mut broken = Vec::new();
    // Each run asks for a stop at its very end, after which it takes no step: it ends with the
    // host function's value, or its error. Either way it's over, and the next run starts clean.
    let stop_now = combiner(&mut g, &mut s, "stop-now");
    let stop_and_fail = combiner(&mut g, &mut s, "stop-and-fail");
    for (what, f) in [("returned", stop_now), ("failed", stop_and_fail)] {
        let _ = g.call(&mut s, f, &[]);
        let r = g.evaluate(&mut s, "(add 1 2)");
        let got = outcome(&g, &r);
        if got != "ok 3" {
            broken.push(format!(
                "after a run that asked for a stop and {what}, the next run gave {got}"
            ));
        }
    }
    report("a stop during one run never reaches the next", &broken);
}
