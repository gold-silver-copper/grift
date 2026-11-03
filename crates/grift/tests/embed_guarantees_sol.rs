#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use grift::{Error, Grift, Primitive, Value};
use std::sync::atomic::{AtomicBool, Ordering};

fn grift<T>() -> Grift<T> {
    Grift::new().expect("the library loads")
}

fn printed<T>(g: &Grift<T>, value: Value) -> String {
    let mut text = String::new();
    g.write(value, &mut text).unwrap();
    text
}

fn outcome<T>(g: &Grift<T>, result: &Result<Value, Error>) -> String {
    match result {
        Ok(value) => format!("returned {value:?}"),
        Err(error) => format!("error {}", printed(g, Value::Symbol(error.kind))),
    }
}

#[derive(Clone, Copy)]
struct Objects {
    pair: Value,
    string: Value,
    other_string: Value,
    context: Value,
    operative: Value,
    applicative: Value,
}

impl Objects {
    fn entries(self) -> [(&'static str, Value); 6] {
        [
            ("pair", self.pair),
            ("string", self.string),
            ("other string", self.other_string),
            ("context", self.context),
            ("operative", self.operative),
            ("applicative", self.applicative),
        ]
    }
}

fn objects(g: &mut Grift, marker: i64) -> Objects {
    let operative = g
        .evaluate(&mut (), &format!("(vau! () #ignore {marker})"))
        .unwrap();
    let pair = g.pair(Value::Integer(marker), Value::EmptyList).unwrap();
    let string = g.string(&format!("string {marker}")).unwrap();
    let other_string = g.string("different contents").unwrap();
    let context = g.context().unwrap();
    let applicative = g
        .host_function("return-inert", |_, _, _| Ok(Value::Inert))
        .unwrap();
    Objects {
        pair,
        string,
        other_string,
        context,
        operative,
        applicative,
    }
}

fn reclaimed() -> (Grift, Objects) {
    let mut g = grift();
    let old = objects(&mut g, 7);
    assert!(g.as_pair(old.pair).is_some());
    assert!(g.as_str(old.string).is_some());
    assert_ne!(g.as_str(old.string), g.as_str(old.other_string));
    assert_eq!(g.as_pair(Value::Integer(0)), None);
    assert_eq!(g.as_str(Value::Integer(0)), None);

    // There is no public collect operation. Allocate a bounded batch of garbage, then give
    // the machine a safe point. Prove reclamation through readers that already preserve the
    // error, rather than using either lossy Option accessor as evidence of collection.
    for _ in 0..32_768 {
        g.pair(Value::Inert, Value::EmptyList).unwrap();
    }
    g.evaluate(&mut (), "#inert").unwrap();
    let stale = g.symbol("stale-value").unwrap();
    for (primitive, args) in [
        (Primitive::First, vec![old.pair]),
        (Primitive::ReadForm, vec![old.string, Value::Integer(0)]),
        (
            Primitive::ReadForm,
            vec![old.other_string, Value::Integer(0)],
        ),
        (Primitive::Names, vec![old.context]),
    ] {
        let error = g
            .call(&mut (), Value::Primitive(primitive), &args)
            .unwrap_err();
        assert_eq!(error.kind, stale, "setup must reclaim {}", primitive.name());
    }
    for value in [old.operative, old.applicative] {
        let error = g.call(&mut (), value, &[]).unwrap_err();
        assert_eq!(error.kind, stale, "setup must reclaim the combiner");
    }
    (g, old)
}

fn stale_failures(g: &mut Grift, cases: Vec<(Primitive, Vec<Value>)>) -> Vec<String> {
    let stale = g.symbol("stale-value").unwrap();
    let mut failures = Vec::new();
    for (primitive, arguments) in cases {
        let result = g.call(&mut (), Value::Primitive(primitive), &arguments);
        if !matches!(&result, Err(error) if error.kind == stale) {
            failures.push(format!(
                "{} {arguments:?}: {}",
                primitive.name(),
                outcome(g, &result)
            ));
        }
    }
    failures
}

#[test]
fn rust_accessors_distinguish_reclaimed_values_from_wrong_types() {
    let (g, old) = reclaimed();
    // Today's Option cannot express stale-value. This necessary distinction fails today;
    // adapt it to assert Err(stale-value) when these readers gain an error channel.
    let indistinguishable = [
        ("as_pair", g.as_pair(old.pair) == g.as_pair(Value::Inert)),
        ("as_str", g.as_str(old.string) == g.as_str(Value::Inert)),
    ];
    assert!(
        indistinguishable.iter().all(|(_, same)| !same),
        "Rust readers erased stale-value into the ordinary wrong-type answer: {indistinguishable:?}"
    );
}

#[test]
fn writing_a_reclaimed_value_must_report_an_error() {
    let (g, old) = reclaimed();
    let mut accepted = Vec::new();
    for (name, value) in old.entries() {
        let mut text = String::new();
        if g.write(value, &mut text).is_ok() {
            accepted.push(format!("{name}: {text:?}"));
        }
    }
    // fmt::Result cannot identify stale-value yet. The writer will adapt this assertion to
    // check the interpreter error kind once write can return one.
    assert!(
        accepted.is_empty(),
        "write accepted reclaimed values instead of reporting stale-value: {accepted:?}"
    );
}

#[test]
fn primitive_argument_errors_must_preserve_stale_value() {
    let (mut g, old) = reclaimed();
    // Exercise the lossy text reader, tag-based type checks, the parameter-tree check and
    // the range check that can bypass reading its string. Invalid references take precedence
    // over each ordinary argument error, regardless of the expected type or argument slot.
    let failures = stale_failures(
        &mut g,
        vec![
            (Primitive::StringToSymbol, vec![old.string]),
            (Primitive::Add, vec![old.pair, Value::Integer(0)]),
            (Primitive::Add, vec![Value::Integer(0), old.context]),
            (Primitive::Sine, vec![old.pair]),
            (Primitive::First, vec![old.string]),
            (Primitive::Unwrap, vec![old.pair]),
            (
                Primitive::Vau,
                vec![old.string, Value::Ignore, Value::Inert],
            ),
            (
                Primitive::ByteSubstring,
                vec![old.string, Value::Integer(-1), Value::Integer(0)],
            ),
        ],
    );
    assert!(
        failures.is_empty(),
        "primitive argument checks replaced stale-value: {failures:#?}"
    );
}

#[test]
fn primitives_cannot_answer_from_reclaimed_tags_or_identity() {
    let (mut g, old) = reclaimed();
    let live_context = g.context().unwrap();
    // These paths need not dereference the value to compute their current answer. They
    // must still validate it: two freed strings are not equal because both readers gave None.
    let mut cases = vec![
        (Primitive::Same, vec![old.string, old.other_string]),
        (Primitive::Same, vec![old.pair, old.pair]),
        (
            Primitive::Select,
            vec![Value::Boolean(true), old.string, Value::Inert],
        ),
        (Primitive::Pair, vec![old.string, Value::EmptyList]),
        (Primitive::Wrap, vec![old.operative]),
        (Primitive::Context, vec![old.context]),
        (Primitive::Evaluate, vec![old.string, live_context]),
        (Primitive::ValueToString, vec![old.string]),
    ];
    cases.extend(
        old.entries()
            .into_iter()
            .map(|(_, value)| (Primitive::TypeOf, vec![value])),
    );
    let failures = stale_failures(&mut g, cases);
    assert!(
        failures.is_empty(),
        "primitives answered using reclaimed arguments instead of stale-value: {failures:#?}"
    );
}

#[test]
fn foreign_heap_values_are_never_read_as_receiver_objects() {
    let (mut source, mut receiver) = (grift(), grift());
    // The same allocation history makes slot collisions likely without inspecting or
    // requiring any particular index, generation, representation or future owner id.
    let foreign = objects(&mut source, 1);
    let local = objects(&mut receiver, 2);
    for (name, value) in local.entries() {
        receiver.define(name, value).unwrap();
    }
    assert!(receiver.as_pair(local.pair).is_some());
    assert!(receiver.as_str(local.string).is_some());
    let mut accepted = Vec::new();
    // Adapt Option readers to assert the foreign-value error once they can return errors.
    if let Some(pair) = receiver.as_pair(foreign.pair) {
        accepted.push(format!("as_pair read a receiver pair: {pair:?}"));
    }
    if let Some(text) = receiver.as_str(foreign.string) {
        accepted.push(format!("as_str read a receiver string: {text:?}"));
    }
    for (name, value) in foreign.entries() {
        let mut text = String::new();
        if receiver.write(value, &mut text).is_ok() {
            accepted.push(format!("write read foreign {name} as {text:?}"));
        }
    }
    for (name, combiner, args) in [
        (
            "context lookup",
            Value::Primitive(Primitive::Names),
            vec![foreign.context],
        ),
        ("operative call", foreign.operative, vec![]),
        ("applicative call", foreign.applicative, vec![]),
    ] {
        let result = receiver.call(&mut (), combiner, &args);
        if result.is_ok() {
            accepted.push(format!("{name}: {}", outcome(&receiver, &result)));
        }
    }
    assert!(
        accepted.is_empty(),
        "foreign heap values aliased receiver objects instead of errors: {accepted:#?}"
    );
}

#[test]
fn a_foreign_host_id_must_not_invoke_a_receiver_function() {
    fn counted(_: &mut Grift<usize>, calls: &mut usize, _: &[Value]) -> Result<Value, Error> {
        *calls += 1;
        Ok(Value::Inert)
    }
    let (mut source, mut receiver) = (grift(), grift());
    let source_function = source.host_function("source-function", counted).unwrap();
    let foreign_host = source
        .call(
            &mut 0,
            Value::Primitive(Primitive::Unwrap),
            &[source_function],
        )
        .unwrap();
    assert!(matches!(foreign_host, Value::Host(_)));
    receiver
        .host_function("receiver-function", counted)
        .unwrap();
    let mut calls = 0;
    let result = receiver.call(&mut calls, foreign_host, &[]);
    assert!(
        result.is_err() && calls == 0,
        "foreign host id invoked receiver code: {calls} calls, {}",
        outcome(&receiver, &result)
    );
}

#[test]
fn foreign_symbols_are_never_read_as_receiver_names() {
    let (mut source, mut receiver): (Grift, Grift) = (grift(), grift());
    let foreign = source.symbol("source-only-name").unwrap();
    let local = receiver.symbol("receiver-only-name").unwrap();
    let scope = receiver.context().unwrap();
    receiver
        .call(
            &mut (),
            Value::Primitive(Primitive::Bind),
            &[scope, Value::Symbol(local), Value::Integer(9)],
        )
        .unwrap();
    let mut text = String::new();
    let written = receiver.write(Value::Symbol(foreign), &mut text);
    let looked_up = receiver.call(
        &mut (),
        Value::Primitive(Primitive::Lookup),
        &[scope, Value::Symbol(foreign), Value::Inert],
    );
    // write's current fmt::Result can only express failure, not its interpreter error kind.
    assert!(
        written.is_err() && looked_up.is_err(),
        "foreign symbol became a receiver name: write={written:?} text={text:?}, lookup {}",
        outcome(&receiver, &looked_up)
    );
}

static IDLE_STOP: AtomicBool = AtomicBool::new(false);

#[test]
fn a_stop_requested_while_idle_cannot_interrupt_a_later_run() {
    let mut g: Grift = grift();
    g.set_interrupt_flag(&IDLE_STOP);
    let expected = g.evaluate(&mut (), "(add 20 22)").unwrap();
    // Today's flag store stands in for Interrupter::stop(). Adapt only the stop API.
    IDLE_STOP.store(true, Ordering::Relaxed);
    let later = g.evaluate(&mut (), "(add 20 22)");
    assert_eq!(
        later,
        Ok(expected),
        "a stop with no run in progress reached a later evaluation: {}",
        outcome(&g, &later)
    );
}

static EXIT_STOP: AtomicBool = AtomicBool::new(false);

#[test]
fn a_stop_at_host_exit_cannot_escape_into_the_next_run() {
    fn stop_at_exit(g: &mut Grift<bool>, fail: &mut bool, _: &[Value]) -> Result<Value, Error> {
        EXIT_STOP.store(true, Ordering::Relaxed);
        if *fail {
            Err(g.error("host-failed", &[]))
        } else {
            Ok(Value::Inert)
        }
    }
    let mut g = grift();
    let f = g.host_function("stop-at-exit", stop_at_exit).unwrap();
    g.define("stop-at-exit", f).unwrap();
    g.set_interrupt_flag(&EXIT_STOP);

    // Already holds: when another step observes the stop, it is consumed and the next run
    // succeeds. The failure below concerns exits with no subsequent safe point.
    let interrupted = g
        .evaluate(&mut false, "(sequence! (stop-at-exit) #inert)")
        .unwrap_err();
    assert_eq!(interrupted.kind, g.symbol("interrupted").unwrap());
    assert!(!EXIT_STOP.load(Ordering::Relaxed));
    let expected = g.evaluate(&mut false, "(add 20 22)").unwrap();

    let mut leaked = Vec::new();
    for fail in [false, true] {
        let mut state = fail;
        let first = g.call(&mut state, f, &[]);
        let first = outcome(&g, &first);
        let later = g.evaluate(&mut state, "(add 20 22)");
        if later != Ok(expected) {
            leaked.push(format!(
                "host fail={fail}, first {first}, next {}",
                outcome(&g, &later)
            ));
        }
    }
    // Adapt flag stores to Interrupter::stop(); keep both successful and error exits.
    assert!(
        leaked.is_empty(),
        "a stop during a completed host call interrupted the next run: {leaked:#?}"
    );
}
