//! How a run ends: who began the unwinding decides what may catch it, never what its kind is called.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use crate::parked_tests::{give_up, parked, start};
use crate::tests::kind;
use crate::{Answer, Error, Grift, Interrupter, Ran, Value};

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
    for name in ["interrupted", "out-of-memory", "exit"] {
        for raised in [
            format!("(raise (error (quote! {name})))"),
            format!("(reports \"{name}\")"),
        ] {
            let caught = format!("(catch! {raised} e (evaluate-in! kind e))");
            let v = g.evaluate(&mut s, &caught).unwrap();
            assert_eq!(v, Value::Symbol(g.symbol(name).unwrap()), "{raised}");
            // Not caught, it reaches the host as what it is: data with that name.
            let e = g.evaluate(&mut s, &raised).unwrap_err();
            assert!(
                g.symbol_name(&e.kind).unwrap() == name && !e.limit,
                "{raised}"
            );
        }
    }
}

#[test]
fn a_stop_passes_handlers_and_the_host_can_tell() {
    let (mut g, mut s) = host();
    let e = g
        .evaluate(&mut s, "(catch! (sequence! (stop) (add 1 2)) #ignore 0)")
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
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
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("out-of-memory", true));
    assert_eq!(g.evaluate(&mut s, "caught"), Ok(Value::Boolean(false)));
}

/// A limit a host function returns stays the host's when its values were reclaimed and it can't
/// come in: it is `stale-value`, and still a limit.
#[test]
fn a_limit_from_a_host_function_stays_one_even_if_it_can_no_longer_come_in() {
    let mut g: Grift<Option<Error>> = Grift::new().unwrap();
    let late = g
        .host_function("late", |_, s, _| Err(s.take().unwrap()))
        .unwrap();
    g.define("late", late).unwrap();
    let held = g.string("reclaimed").unwrap();
    let mut s = Some(Error {
        limit: true,
        ..g.error("ended", &[held])
    });
    g.heap.request_collection();
    g.evaluate(&mut None, "#inert").unwrap();
    let e = g.evaluate(&mut s, "(catch! (late) #ignore 0)").unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("stale-value", true));
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
            (kind(&g, &e).as_str(), e.limit),
            ("interrupted", true),
            "{raised}"
        );
        let src = format!("(catch! (finally! (wait) {raised}) #ignore 0)");
        let p = parked(start(&mut g, &mut s, &src));
        let e = give_up(&mut g, &mut s, p);
        assert_eq!(
            (kind(&g, &e).as_str(), e.limit),
            ("given-up", true),
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
/// `src`'s `(watch)` stops the run there, so the stop can't land before the cleanups are installed,
/// and leaves a handle for the run to force it a little later.
fn forced(g: &mut Grift<Interrupter>, s: &mut Interrupter, src: &str) {
    let watch = g
        .host_function("watch", |g, _, _| {
            let mut watchdog = g.interrupter();
            assert!(watchdog.stop());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(30));
                watchdog.force();
            });
            Ok(Answer::Now(Value::Inert))
        })
        .unwrap();
    g.define("watch", watch).unwrap();
    let e = g.evaluate(s, src).unwrap_err();
    assert_eq!((kind(g, &e).as_str(), e.limit), ("interrupted", true));
    assert_eq!(log(g, s), "()", "a cleanup finished or began");
}

#[test]
fn a_force_cuts_a_cleanup_that_never_ends() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    forced(
        &mut g,
        &mut s,
        "(finally! (finally! (sequence! (watch) (spin 100000000)) (spin 100000000) (note 1))
                   (note 2))",
    );
}

#[test]
fn a_force_cuts_a_cleanup_that_never_ends_in_a_call_made_for_a_host_function() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    forced(
        &mut g,
        &mut s,
        "(finally! (on-machine (lambda! ()
           (finally! (sequence! (watch) (spin 100000000)) (spin 100000000) (note 1))))
         (note 2))",
    );
}

#[test]
fn a_parked_runs_cleanups_under_giving_up_end_at_a_force() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let p = parked(start(
        &mut g,
        &mut s,
        "(finally! (finally! (wait) (spin 100000000) (note 1)) (spin 100000000) (note 2))",
    ));
    let watchdog = stop_then_force(g.interrupter());
    let e = give_up(&mut g, &mut s, p);
    watchdog.join().unwrap();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
    assert_eq!(log(&mut g, &mut s), "()");
}

/// The nearest cleanup of a run that ran out of memory gets what the frames dropped let go of.
#[test]
fn memory_the_frames_held_is_there_for_the_nearest_cleanup() {
    let (mut g, mut s) = host();
    g.evaluate(
        &mut s,
        &format!("{LOG} (define! down (lambda! (n) (pair n (down (add n 1)))))"),
    )
    .unwrap();
    g.heap.slot_limit = g.heap.len() + 2000;
    let e = g
        .evaluate(&mut s, "(finally! (finally! (down 0) (note 1)) (note 2))")
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("out-of-memory", true));
    g.heap.slot_limit = usize::MAX;
    assert_eq!(log(&mut g, &mut s), "(2 1)");
}

/// A name decides nothing: only memory really refused asks for a collection.
#[test]
fn an_error_named_out_of_memory_collects_nothing() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, "(raise (error (quote! out-of-memory)))")
        .unwrap_err();
    assert!(!g.heap.due());
    g.evaluate(&mut s, "(reports \"out-of-memory\")")
        .unwrap_err();
    assert!(!g.heap.due());
    // A limit that says so is still only named so: no memory was refused.
    let quit = g
        .host_function("quit", |g, _, _| {
            Err(Error {
                limit: true,
                ..g.error("out-of-memory", &[])
            })
        })
        .unwrap();
    g.define("quit", quit).unwrap();
    g.evaluate(&mut s, "(quit)").unwrap_err();
    assert!(!g.heap.due());
    g.heap.slot_limit = g.heap.len();
    while g.pair(Value::Inert, Value::Inert).is_ok() {}
    assert!(g.heap.due(), "memory refused");
}

#[test]
fn a_stale_limit_given_to_resume_stays_a_limit() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let e = g.evaluate(&mut s, "(stop) (add 1 2)").unwrap_err();
    let p = parked(start(
        &mut g,
        &mut s,
        "(catch! (wait) e (note (quote! caught)))",
    ));
    g.heap.request_collection();
    g.evaluate(&mut s, "#inert").unwrap();
    let ended = g.resume(&mut s, p, Err(e)).unwrap_err().error;
    assert!(ended.limit);
    assert_eq!(log(&mut g, &mut s), "()");
}

#[test]
fn a_force_before_a_run_parks_ends_it() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let wait = g
        .host_function("force-and-wait", |g, _, _| {
            assert!(g.interrupter().force());
            Ok(Answer::Later(Value::Inert))
        })
        .unwrap();
    g.define("force-and-wait", wait).unwrap();
    let top = g.top_level();
    let ran = g.start_in(&mut s, &top, "t", "(finally! (force-and-wait) (note 1))");
    assert_eq!(kind(&g, &ran.unwrap_err()), "interrupted");
    assert_eq!(g.parked(), 0);
    assert_eq!(log(&mut g, &mut s), "()");
}

/// Force, then memory refused where the cleanup would start: the force names the ending.
#[test]
fn a_force_names_the_ending_whatever_memory_refuses_next() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let refuse = g
        .host_function("refuse", |g, _, _| {
            g.heap.refuse_in = Some(0);
            g.heap.refuse_once = true;
            Ok(Answer::Now(Value::Inert))
        })
        .unwrap();
    g.define("refuse", refuse).unwrap();
    let e = g
        .evaluate(&mut s, "(finally! (sequence! (refuse) (force)) (note 1))")
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
    assert_eq!(log(&mut g, &mut s), "()");
}

/// A force in a call made for a host function passes the host function's `then` by: nothing a
/// host function does can swallow it.
#[test]
fn a_host_function_cannot_swallow_a_force_in_a_call_made_for_it() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let e = g
        .evaluate(
            &mut s,
            "(finally! (sequence! (swallow (lambda! () (force))) (note 1)) (note 2))",
        )
        .unwrap_err();
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
    assert_eq!(log(&mut g, &mut s), "()");
}

/// A cleanup's start needs no room it didn't make at its install, however full the stack is then.
#[test]
fn a_cleanups_install_makes_the_room_its_start_needs() {
    let (mut g, mut s) = host();
    let tight = g
        .host_function("tight", |g, _, args| {
            let [Value::Integer(room)] = args else {
                panic!("{args:?}");
            };
            g.machine.leave_room(*room as usize);
            Ok(Answer::Now(Value::Inert))
        })
        .unwrap();
    g.define("tight", tight).unwrap();
    // The primitive itself, on names: nothing between `tight` and the install grows the stack.
    let names = "(define! here (current-context)) (define! then (lambda! () 0))";
    g.evaluate(&mut s, names).unwrap();
    for room in 0..12 {
        let src = format!("(sequence! (tight {room}) (evaluate-with-cleanup 1 here then))");
        g.evaluate(&mut s, &src).unwrap();
    }
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
            seen.push(g.heap.collections);
            Ok(Answer::Now(Value::Inert))
        })
        .unwrap();
    g.define("seen", seen).unwrap();
    g.evaluate(
        &mut collections,
        "(define! down (lambda! (n) (finally! (down (add n 1)) (seen))))",
    )
    .unwrap();
    g.heap.slot_limit = g.heap.len() + 20_000;
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

/// A host keeps a limit and hands it in later, from a host function or as an answer to a wait: it
/// still passes every handler, and an ordinary error kept the same way keeps its kind and values.
#[test]
fn what_a_host_keeps_comes_back_what_it_was() {
    let (mut g, mut s) = host();
    g.evaluate(&mut s, LOG).unwrap();
    let mut limit = g.evaluate(&mut s, "(stop) (add 1 2)").unwrap_err();
    // Its trace's combinations are garbage once the run is over: a collection before it comes back
    // would make it stale-value, as `Error::limit` says. The kind and the limit are the point.
    limit.trace.clear();
    let p = parked(start(
        &mut g,
        &mut s,
        "(catch! (wait) #ignore (note (quote! caught)))",
    ));
    let e = g.resume(&mut s, p, Err(limit.clone())).unwrap_err().error;
    assert_eq!((kind(&g, &e).as_str(), e.limit), ("interrupted", true));
    let ordinary = g
        .evaluate(&mut s, "(raise (error (quote! ordinary) 123))")
        .unwrap_err();
    let p = parked(start(
        &mut g,
        &mut s,
        "(catch! (wait) e (list (evaluate-in! kind e) (evaluate-in! values e)))",
    ));
    let Ran::Finished(v) = g.resume(&mut s, p, Err(ordinary)).unwrap() else {
        panic!("it finishes");
    };
    let mut text = String::new();
    g.write(&v, &mut text).unwrap();
    assert_eq!(text, "(ordinary (123))");
    assert_eq!(log(&mut g, &mut s), "()");
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
