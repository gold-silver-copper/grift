//! How a run ends: who began the unwinding decides what may catch it, never what its kind is called.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use crate::tests::kind;
use crate::{Answer, Grift, Interrupter, Value};

/// A host whose program can stop its own run and force it, with a run that waits.
fn host() -> (Grift<Interrupter>, Interrupter) {
    let mut g: Grift<Interrupter> = Grift::new().unwrap();
    let functions: [(&str, crate::HostFunction<Interrupter>); 6] = [
        // Each stops or forces the run it is called in, with a handle for that run.
        ("stop", |g, _, _| {
            g.interrupter().stop();
            Ok(Answer::Now(Value::Inert))
        }),
        ("force", |g, _, _| {
            Ok(Answer::Now(Value::Boolean(g.interrupter().force())))
        }),
        // `(on-machine thunk)`: the machine calls `thunk` for this host function.
        ("on-machine", |_, _, args| {
            Ok(Answer::Call {
                function: args[0].clone(),
                arguments: Vec::new(),
                then: None,
            })
        }),
        // `(swallow thunk)`: calls `thunk`, then would give `(list ending)`, whatever it ends with.
        ("swallow", |g, _, args| {
            let (top, list) = (g.top_level(), g.symbol("list")?);
            Ok(Answer::Call {
                function: args[0].clone(),
                arguments: Vec::new(),
                then: Some((g.lookup(&top, &list)?.unwrap(), Vec::new())),
            })
        }),
        ("reports", |g, _, args| {
            let name = g.as_str(&args[0])?.unwrap().to_owned();
            Err(g.error(&name, &[]))
        }),
        ("wait", |_, _, _| Ok(Answer::Later(Value::Inert))),
    ];
    for (name, f) in functions {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    let s = g.interrupter();
    (g, s)
}

/// Memory refused to a machine stack or a host's value, not by the heap: what can be let go of is
/// worth collecting all the same, for the cleanups or for the host's next try.
#[test]
fn memory_refused_outside_the_heap_asks_for_a_collection() {
    let (mut g, _) = host();
    g.heap.not_due();
    let _ = g.unwind(crate::local::Error::oom());
    assert!(g.heap.due());
    g.heap.not_due();
    let _ = g.export_error(crate::local::Error::oom());
    assert!(g.heap.due());
}

/// A refusal asks for one collection, not one for each cleanup it goes through: each marks all the
/// levels left, so that would take time in the square of the depth. Only a new refusal asks again.
#[test]
fn memory_refused_collects_once_however_many_cleanups_it_passes() {
    let mut g: Grift<Vec<usize>> = Grift::new().unwrap();
    let mut collections = Vec::new();
    let seen = g
        .host_function("seen", |g, seen: &mut Vec<usize>, _| {
            seen.push(g.heap.probe.collections);
            Ok(Answer::Now(Value::Inert))
        })
        .unwrap();
    g.define("seen", seen).unwrap();
    g.evaluate(
        &mut collections,
        "(define! down (lambda! (n) (finally! (down (add n 1)) (seen))))",
    )
    .unwrap();
    g.heap.probe.slot_limit = Some(g.heap.len() + 20_000);
    let e = g.evaluate(&mut collections, "(down 0)").unwrap_err();
    assert_eq!(kind(&g, &e), "out-of-memory");
    assert!(collections.len() > 500, "{} cleanups", collections.len());
    let made = collections[collections.len() - 1] - collections[0];
    assert!(
        made < 10,
        "{made} collections, {} cleanups",
        collections.len()
    );
}

/// A handle reaches the one run it was taken for: the run in progress, or, taken between runs,
/// the next to begin. So no handle, fresh or old, stop or force, or a clone of one, reaches a run
/// its host didn't mean, and a force needs no stop before it.
#[test]
fn a_handle_reaches_only_the_run_it_was_taken_for() {
    let mut g: Grift<Vec<Interrupter>> = Grift::new().unwrap();
    let functions: [(&str, crate::HostFunction<Vec<Interrupter>>); 5] = [
        ("panics", |_, _, _| panic!("a host function's bug")),
        ("take", |g, s, _| {
            s.push(g.interrupter());
            Ok(Answer::Now(Value::Inert))
        }),
        ("stop-each", |_, s, _| {
            let said = s.iter_mut().map(|h| h.stop()).filter(|&b| b).count();
            Ok(Answer::Now(Value::Integer(said as i64)))
        }),
        ("force-each", |_, s, _| {
            let said = s.iter_mut().map(|h| h.force()).filter(|&b| b).count();
            Ok(Answer::Now(Value::Integer(said as i64)))
        }),
        ("force-clones", |_, s, _| {
            let said = s.iter().map(|h| h.clone().force()).filter(|&b| b).count();
            Ok(Answer::Now(Value::Integer(said as i64)))
        }),
    ];
    for (name, f) in functions {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    let mut s = vec![g.interrupter()];
    assert!(s.iter_mut().all(|h| !h.force()), "its run has not begun");
    assert_eq!(g.evaluate(&mut s, "(add 1 2)"), Ok(Value::Integer(3)));
    assert_eq!(g.evaluate(&mut s, "(take)"), Ok(Value::Inert));
    // One was taken for the first run, one inside the second: both are over, and their clones.
    let v = g
        .evaluate(
            &mut s,
            "(list (stop-each) (force-each) (force-clones) (add 1 2))",
        )
        .unwrap();
    assert_eq!(
        g.list_items(&v).unwrap(),
        [
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(3)
        ]
    );
    assert!(s.iter_mut().all(|h| !h.stop() && !h.force()));
    // One taken between runs reaches the next, and a force alone ends it, cleanups and all.
    g.evaluate(&mut s, "(define! cleaned 0)").unwrap();
    s.push(g.interrupter());
    let e = g
        .evaluate(
            &mut s,
            "(finally! (force-each) (bind (current-context) (quote! cleaned) 1))",
        )
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
    // A force that acted is over with its run: the next is untouched, and the handle says so.
    assert_eq!(g.evaluate(&mut s, "cleaned"), Ok(Value::Integer(0)));
    assert!(s.iter_mut().all(|h| !h.force()));
    // A clone of one taken inside a run ends that run.
    let e = g
        .evaluate(&mut s, "(take) (force-clones) (add 1 2)")
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
    // One taken inside a run reaches that run.
    let e = g
        .evaluate(&mut s, "(take) (stop-each) (add 1 2)")
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
    // A run a host function's panic ended leaves no machine behind and ends once, so one taken
    // after it is for the next run.
    let panicked = std::panic::AssertUnwindSafe(|| g.evaluate(&mut s, "(take) (panics)"));
    assert!(std::panic::catch_unwind(panicked).is_err());
    s.push(g.interrupter());
    let e = g.evaluate(&mut s, "(stop-each)").unwrap_err();
    assert_eq!(kind(&g, &e), "interrupted");
}
