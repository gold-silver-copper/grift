//! How a run ends: who began the unwinding decides what may catch it, never what its kind is called.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use crate::{Answer, Error, Grift, Interrupter, Parked, Ran, Value};

fn kind<T>(g: &Grift<T>, e: &Error) -> String {
    g.symbol_name(&e.kind).unwrap().to_string()
}

/// A host whose program can stop its own run and force it, with a run that waits.
fn host() -> (Grift<Interrupter>, Interrupter) {
    let mut g: Grift<Interrupter> = Grift::new().unwrap();
    let functions: [(&str, crate::HostFunction<Interrupter>); 5] = [
        ("stop", |_, s, _| {
            s.stop();
            Ok(Value::Inert)
        }),
        ("force", |_, s, _| Ok(Value::Boolean(s.force()))),
        ("nested", |g, s, args| {
            let src = g.as_string(&args[0])?.unwrap();
            g.evaluate(s, &src)
        }),
        // Returns a nested stop's error after a collection has made its trace stale.
        ("late", |g, s, _| {
            let e = g.evaluate(s, "(stop) (add 1 2)").unwrap_err();
            g.heap.request_collection();
            g.evaluate(s, "#inert")?;
            Err(e)
        }),
        ("reports", |g, _, args| {
            let name = g.as_string(&args[0])?.unwrap();
            Err(g.error(&name, &[]))
        }),
    ];
    for (name, f) in functions {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    let wait = g
        .parking_function("wait", |_, _, _| Ok(Answer::Later(Value::Inert)))
        .unwrap();
    g.define("wait", wait).unwrap();
    let s = g.interrupter();
    (g, s)
}

const LOG: &str = "(define! log ())
    (define! here (current-context))
    (define! note (lambda! (x) (bind here (quote! log) (pair x log))))
    ; Ends by itself, long after a test that works has cut it.
    (define! spin (lambda! (n) (if! (same? n 0) #inert (spin (subtract n 1)))))";

fn log(g: &mut Grift<Interrupter>, s: &mut Interrupter) -> String {
    let v = g.evaluate(s, "log").unwrap();
    let mut text = String::new();
    g.write(&v, &mut text).unwrap();
    text
}

/// Stops the run in progress as soon as there is one, and then forces it, from another thread.
fn stop_then_force(mut h: Interrupter) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !h.stop() {
            std::thread::yield_now();
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        h.force();
    })
}

#[test]
fn a_program_or_a_host_function_can_only_make_what_is_caught() {
    let (mut g, mut s) = host();
    for name in ["interrupted", "out-of-memory", "abandoned"] {
        for raised in [
            format!("(raise (error (quote! {name})))"),
            format!("(reports \"{name}\")"),
        ] {
            let caught = format!("(catch! {raised} e (evaluate-in! kind e))");
            let v = g.evaluate(&mut s, &caught).unwrap();
            assert_eq!(v, Value::Symbol(g.symbol(name).unwrap()), "{raised}");
            // Not caught, it reaches the host as what it is: data with that name.
            let e = g.evaluate(&mut s, &raised).unwrap_err();
            assert!(g.is_kind(&e, name).unwrap() && !e.is_limit(), "{raised}");
        }
    }
}

#[test]
fn a_stop_passes_handlers_and_the_host_can_tell() {
    let (mut g, mut s) = host();
    let e = g
        .evaluate(&mut s, "(catch! (sequence! (stop) (add 1 2)) #ignore 0)")
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.is_limit()), ("interrupted", true));
}

#[test]
fn running_out_of_memory_passes_handlers_and_the_host_can_tell() {
    let (mut g, mut s) = host();
    g.evaluate(
        &mut s,
        "(define! grow (lambda! (x) (grow (pair x x)))) (define! caught #false)",
    )
    .unwrap();
    g.heap.slot_limit = g.heap.len() + 5000;
    let e = g
        .evaluate(
            &mut s,
            "(catch! (grow ()) #ignore (bind (current-context) (quote! caught) #true))",
        )
        .unwrap_err();
    assert_eq!(
        (kind(&g, &e).as_str(), e.is_limit()),
        ("out-of-memory", true)
    );
    assert_eq!(g.evaluate(&mut s, "caught"), Ok(Value::Boolean(false)));
}

fn parked(g: &mut Grift<Interrupter>, s: &mut Interrupter, src: &str) -> Parked {
    let top = g.top_level();
    match g.start_in(s, &top, "test", src) {
        Ok(Ran::Parked(p)) => p,
        other => panic!("{other:?}"),
    }
}

#[test]
fn abandoning_passes_handlers_runs_cleanups_and_the_host_can_tell() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let p = parked(
        &mut g,
        &mut s,
        "(catch! (finally! (wait) (note 1)) #ignore (note 2))",
    );
    let e = g.abandon(&mut s, p).unwrap();
    assert_eq!((kind(&g, &e).as_str(), e.is_limit()), ("abandoned", true));
    assert_eq!(log(&mut g, &mut s), "(1)");
}

#[test]
fn a_nested_runs_stop_stays_the_hosts_through_a_host_function() {
    let (mut g, mut s) = host();
    let e = g
        .evaluate(&mut s, r#"(catch! (nested "(stop) (add 1 2)") #ignore 0)"#)
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.is_limit()), ("interrupted", true));
}

#[test]
fn a_nested_runs_stop_stays_the_hosts_even_if_it_can_no_longer_come_in() {
    let (mut g, mut s) = host();
    let e = g.evaluate(&mut s, "(catch! (late) #ignore 0)").unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.is_limit()), ("stale-value", true));
}

#[test]
fn a_program_in_a_cleanup_cannot_replace_what_the_host_began() {
    let (mut g, mut s) = host();
    for raised in [
        "(first 1)",
        "(raise (error (quote! boom)))",
        "(raise (error (quote! out-of-memory)))",
    ] {
        let src = format!("(catch! (finally! (stop) {raised}) #ignore 0)");
        let e = g.evaluate(&mut s, &src).unwrap_err();
        assert_eq!(
            (kind(&g, &e).as_str(), e.is_limit()),
            ("interrupted", true),
            "{raised}"
        );
        let src = format!("(catch! (finally! (wait) {raised}) #ignore 0)");
        let p = parked(&mut g, &mut s, &src);
        let e = g.abandon(&mut s, p).unwrap();
        assert_eq!(
            (kind(&g, &e).as_str(), e.is_limit()),
            ("abandoned", true),
            "{raised}"
        );
    }
    // Between a program's own errors, whatever they are called, the newest wins.
    let v = g
        .evaluate(
            &mut s,
            "(catch! (finally! (raise (error (quote! interrupted))) (raise (error (quote! boom))))
                     e (evaluate-in! kind e))",
        )
        .unwrap();
    assert_eq!(v, Value::Symbol(g.symbol("boom").unwrap()));
}

/// A force ends `src`, whose cleanups would otherwise take far longer, and no cleanup past it runs.
fn forced(g: &mut Grift<Interrupter>, s: &mut Interrupter, src: &str) {
    let watchdog = stop_then_force(g.interrupter());
    let e = g.evaluate(s, src).unwrap_err();
    watchdog.join().unwrap();
    assert_eq!((kind(g, &e).as_str(), e.is_limit()), ("interrupted", true));
    assert_eq!(log(g, s), "()", "a cleanup finished or began");
}

#[test]
fn a_force_cuts_a_cleanup_that_never_ends() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    forced(
        &mut g,
        &mut s,
        "(finally! (finally! (spin 100000000) (spin 100000000) (note 1)) (note 2))",
    );
}

#[test]
fn a_force_cuts_a_cleanup_that_never_ends_in_a_nested_run() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    forced(
        &mut g,
        &mut s,
        r#"(finally! (nested "(finally! (spin 100000000) (spin 100000000) (note 1))") (note 2))"#,
    );
}

#[test]
fn a_force_never_reaches_a_later_run() {
    let (mut g, mut s) = host();
    assert!(
        !s.force(),
        "this handle stopped nothing, and no run is in progress"
    );
    // One this handle stopped is over: it reaches neither the gap nor the next run.
    let e = g.evaluate(&mut s, "(stop) (add 1 2)").unwrap_err();
    assert_eq!(kind(&g, &e), "interrupted");
    assert!(!s.force(), "that run is over");
    let v = g.evaluate(&mut s, "(list (force) (add 1 2))").unwrap();
    assert_eq!(
        g.list_items(&v).unwrap(),
        [Value::Boolean(false), Value::Integer(3)]
    );
    // One that acted, and stays asked to the end of its run, is over with it.
    let e = g.evaluate(&mut s, "(stop) (force)").unwrap_err();
    assert_eq!(kind(&g, &e), "interrupted");
    assert_eq!(g.evaluate(&mut s, "(add 1 2)"), Ok(Value::Integer(3)));
    assert!(!s.force());
}

#[test]
fn a_parked_runs_cleanups_under_abandon_end_at_a_force() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let p = parked(
        &mut g,
        &mut s,
        "(finally! (finally! (wait) (spin 100000000) (note 1)) (spin 100000000) (note 2))",
    );
    let watchdog = stop_then_force(g.interrupter());
    let e = g.abandon(&mut s, p).unwrap();
    watchdog.join().unwrap();
    assert_eq!((kind(&g, &e).as_str(), e.is_limit()), ("interrupted", true));
    assert_eq!(log(&mut g, &mut s), "()");
}
