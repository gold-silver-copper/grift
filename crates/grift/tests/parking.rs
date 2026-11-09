//! Runs that park: a host function answers "not yet", the run is set aside, and the host's loop
//! goes on until it resumes it, with a value or an error.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use grift::{Answer, Error, Grift, Interrupter, Parked, Ran, ResumeError, Value};

#[derive(Default)]
struct World {
    log: Vec<String>,
    stopper: Option<Interrupter>,
    held: Option<Parked>,
    saved: Vec<Value>,
    refused: Vec<String>,
    watchdog: Option<std::thread::JoinHandle<()>>,
}

type G = Grift<World>;

/// Gives a parked run up, as a host does: the waiting call answers a limit, and so does each wait
/// of a cleanup on the way out. What the run ended with; refused, why, the handle back.
fn give_up(g: &mut G, w: &mut World, mut p: Parked) -> Result<Error, ResumeError> {
    loop {
        let ending = Error {
            limit: true,
            ..g.error("given-up", &[])
        };
        match g.resume(w, p, Err(ending)) {
            Ok(Ran::Parked(again)) => p = again,
            Ok(Ran::Finished(v)) => panic!("a limit ended with the value {v:?}"),
            Err(ResumeError {
                error,
                parked: None,
            }) => return Ok(error),
            Err(refused) => return Err(refused),
        }
    }
}

fn wait(_: &mut G, _: &mut World, _: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Later(Value::Inert))
}

/// What `wait` is to a program, with the answer already there: the same call, not parked.
fn answer(_: &mut G, world: &mut World, _: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Now(world.saved.pop().unwrap_or(Value::Inert)))
}

fn note(g: &mut G, world: &mut World, args: &[Value]) -> Result<Answer, Error> {
    for v in args {
        let mut text = String::new();
        g.write(v, &mut text).unwrap();
        world.log.push(text);
    }
    Ok(Answer::Now(Value::Inert))
}

/// Saves its arguments for the host, parks, and stops itself first if the host armed that.
fn wait_with(g: &mut G, world: &mut World, args: &[Value]) -> Result<Answer, Error> {
    world.saved.extend_from_slice(args);
    if let Some(stopper) = &mut world.stopper {
        assert!(stopper.stop());
    }
    assert!(g.can_park());
    Ok(Answer::Later(Value::Inert))
}

/// Tries to evaluate its string argument inside this host call.
fn nest(g: &mut G, world: &mut World, args: &[Value]) -> Result<Answer, Error> {
    let src = g.as_str(&args[0])?.unwrap().to_owned();
    assert!(g.can_park());
    let done = g.evaluate(world, &src);
    assert!(g.can_park());
    done.map(Answer::Now)
}

/// Tries every way to begin or take up a run from inside a host call.
fn meddle(g: &mut G, world: &mut World, _: &[Value]) -> Result<Answer, Error> {
    let top = g.top_level();
    let e = g.start_in(world, &top, "inner", "1").unwrap_err();
    world.refused.push(kind(g, &e));
    let parked = world.held.take().unwrap();
    let refused = g.resume(world, parked, Ok(Value::Inert)).unwrap_err();
    world.refused.push(kind(g, &refused.error));
    let refused = give_up(g, world, refused.parked.unwrap()).unwrap_err();
    world.refused.push(kind(g, &refused.error));
    world.held = refused.parked;
    Ok(Answer::Now(Value::Inert))
}

/// Asks for its argument.
fn ask(_: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Later(args[0].clone()))
}

/// Asks for what the host saved.
fn ask_saved(_: &mut G, world: &mut World, _: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Later(world.saved.pop().unwrap()))
}

fn boom(_: &mut G, _: &mut World, _: &[Value]) -> Result<Answer, Error> {
    panic!("a host function panics");
}

/// The forms of the text in the host's first saved value, as the runner's `read-forms-host`.
fn read_forms(g: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
    let name = g.as_str(&args[0])?.unwrap().to_owned();
    g.read_in(
        &name,
        "(note (quote! loaded))\n(define! from-file (wait))\n(first from-file)",
    )
    .map(Answer::Now)
}

fn grift() -> G {
    let mut g = G::new().unwrap();
    for (name, f) in [
        ("wait", wait as grift::HostFunction<World>),
        ("wait-with", wait_with),
        ("ask", ask),
        ("ask-saved", ask_saved),
        ("on-machine", on_machine),
        ("and-then", and_then),
        ("answer", answer),
        ("note", note),
        ("nest", nest),
        ("meddle", meddle),
        ("boom", boom),
        ("read-forms-host", read_forms),
    ] {
        let f = g.host_function(name, f).unwrap();
        g.define(name, f).unwrap();
    }
    g
}

fn kind<T>(g: &Grift<T>, e: &Error) -> String {
    g.symbol_name(&e.kind).unwrap().to_string()
}

/// What a run came to: the errors of `start_in` and `resume` are one here.
type Done = Result<Ran, Error>;

fn run(g: &mut G, w: &mut World, src: &str) -> Done {
    let top = g.top_level();
    g.start_in(w, &top, "script.grift", src)
}

#[track_caller]
fn parked(ran: Done) -> Parked {
    match ran {
        Ok(Ran::Parked(p)) => p,
        other => panic!("not parked: {other:?}"),
    }
}

#[track_caller]
fn resume(g: &mut G, w: &mut World, p: Parked, v: Value) -> Done {
    went_on(g.resume(w, p, Ok(v)))
}

/// The run was taken up: any error is its end, with no handle left.
#[track_caller]
fn went_on(done: Result<Ran, ResumeError>) -> Done {
    done.map_err(|e| {
        assert!(e.parked.is_none(), "refused: {:?}", e.error);
        e.error
    })
}

#[track_caller]
fn finished(g: &G, ran: Done) -> String {
    match ran {
        Ok(Ran::Finished(v)) => {
            let mut s = String::new();
            g.write(&v, &mut s).unwrap();
            s
        }
        other => panic!("not finished: {other:?}"),
    }
}

#[track_caller]
fn failed(ran: Done) -> Error {
    match ran {
        Err(e) => e,
        other => panic!("not failed: {other:?}"),
    }
}

fn collect(g: &mut G, w: &mut World) {
    g.evaluate(w, "(length (map list (range 0 20000)))")
        .unwrap();
}

fn described(g: &G, e: &Error) -> String {
    let mut s = String::new();
    g.describe(e, &mut s, 4096).unwrap();
    s
}

#[test]
fn a_run_parks_and_takes_a_value_or_an_error_once() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(&mut g, &mut w, "(add 1 (wait))"));
    assert_eq!(g.parked(), 1);
    // The host's loop goes on: the interpreter is free.
    assert_eq!(g.evaluate(&mut w, "(add 2 2)").unwrap(), Value::Integer(4));
    let done = resume(&mut g, &mut w, p, Value::Integer(41));
    assert_eq!(finished(&g, done), "42");
    assert_eq!(g.parked(), 0);
    assert!(g.forgotten().is_none());

    let p = parked(run(
        &mut g,
        &mut w,
        "(catch! (wait) e (evaluate-in! kind e))",
    ));
    let e = g.error("timed-out", &[]);
    let done = went_on(g.resume(&mut w, p, Err(e)));
    assert_eq!(finished(&g, done), "timed-out");

    let p = parked(run(&mut g, &mut w, "(add 1 (wait))"));
    let e = g.error("timed-out", &[Value::Integer(7)]);
    let done = went_on(g.resume(&mut w, p, Err(e)));
    let e = failed(done);
    assert_eq!(
        described(&g, &e),
        "timed-out at script.grift:1 7 in (wait)\n  from (add 1 ...) at script.grift:1"
    );
}

#[test]
fn start_calls_a_function_as_call_does() {
    let (mut g, mut w) = (grift(), World::default());
    let f = g.evaluate(&mut w, "(lambda! (x) (add x (wait)))").unwrap();
    let p = parked(g.start(&mut w, &f, &[Value::Integer(1)]));
    let done = resume(&mut g, &mut w, p, Value::Integer(2));
    assert_eq!(finished(&g, done), "3");
    let e = failed(g.start(&mut w, &Value::Integer(1), &[]));
    assert_eq!(kind(&g, &e), "type-error");
}

#[test]
fn many_runs_share_one_heap_and_finish_in_any_order() {
    let (mut g, mut w) = (grift(), World::default());
    g.evaluate(
        &mut w,
        "(define! total 0) (define! here (current-context))
                        (define! count! (lambda! (n) (bind here (quote! total) (add total n))))",
    )
    .unwrap();
    let a = parked(run(
        &mut g,
        &mut w,
        "(count! 1) (count! (wait)) (count! (wait)) total",
    ));
    let b = parked(run(&mut g, &mut w, "(count! 10) (count! (wait)) total"));
    let done = run(&mut g, &mut w, "(count! 100) total");
    assert_eq!(finished(&g, done), "111");
    let c = parked(run(&mut g, &mut w, "(count! (wait)) total"));
    assert_eq!(g.parked(), 3);
    let a = parked(resume(&mut g, &mut w, a, Value::Integer(1000)));
    let done = resume(&mut g, &mut w, c, Value::Integer(10000));
    assert_eq!(finished(&g, done), "11111");
    let done = resume(&mut g, &mut w, b, Value::Integer(100000));
    assert_eq!(finished(&g, done), "111111");
    let done = resume(&mut g, &mut w, a, Value::Integer(1000000));
    assert_eq!(finished(&g, done), "1111111");
}

#[test]
fn giving_up_runs_every_cleanup_and_nothing_catches_it() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(
        &mut g,
        &mut w,
        "(finally!
           (catch!
             (finally! (sequence! (note 1) (wait) (note 2)) (note 3))
             e (note 4))
           (note 5))",
    ));
    let e = give_up(&mut g, &mut w, p).unwrap();
    assert_eq!(kind(&g, &e), "given-up");
    assert!(e.limit);
    assert_eq!(w.log, ["1", "3", "5"]);
    assert_eq!(g.parked(), 0);
}

#[test]
fn a_cleanup_that_errs_or_waits_during_giving_up_ends_there_and_the_rest_run() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(
        &mut g,
        &mut w,
        "(finally!
           (finally!
             (finally! (wait) (sequence! (note 1) (first 0) (note 2)))
             (sequence! (note 3) (catch! (wait) e (note (evaluate-in! values e))) (wait) (note 4)))
           (note 5))",
    ));
    let e = give_up(&mut g, &mut w, p).unwrap();
    assert_eq!(kind(&g, &e), "given-up");
    // The answer that ended the cleanup's wait is a limit, so the `catch!` around that wait saw
    // nothing and the cleanup ended there.
    assert_eq!(w.log, ["1", "3", "5"]);
    assert_eq!(g.parked(), 0);
}

/// A cleanup met on the way out can wait, as any run can, and the host may answer it as it likes:
/// a value lets the cleanup finish, and the limit is still what the run ends with.
#[test]
fn a_cleanup_that_waits_after_a_limit_is_a_run_that_waits_and_the_limit_stays() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(
        &mut g,
        &mut w,
        "(finally! (wait) (sequence! (note 1) (note (wait)) (note 3)))",
    ));
    let ending = Error {
        limit: true,
        ..g.error("given-up", &[])
    };
    let again = parked(g.resume(&mut w, p, Err(ending)).map_err(Error::from));
    assert_eq!(g.parked(), 1);
    let e = g.resume(&mut w, again, Ok(Value::Integer(2))).unwrap_err();
    assert_eq!(kind(&g, &e.error), "given-up");
    assert!(e.error.limit);
    assert_eq!(w.log, ["1", "2", "3"]);
    assert_eq!(g.parked(), 0);
}

#[test]
fn a_dropped_handle_is_found_again_and_its_run_stays_alive_until_then() {
    let (mut g, mut w) = (grift(), World::default());
    assert!(g.forgotten().is_none());
    let p = parked(run(
        &mut g,
        &mut w,
        "(let! ((held (list 1 2 3))) (finally! (wait) (note held)))",
    ));
    drop(p);
    collect(&mut g, &mut w);
    assert_eq!(g.parked(), 1);
    let p = g.forgotten().unwrap();
    assert!(g.forgotten().is_none());
    give_up(&mut g, &mut w, p).unwrap();
    assert_eq!(w.log, ["(1 2 3)"]);
    assert_eq!(g.parked(), 0);
}

#[test]
fn a_dropped_interpreter_runs_no_cleanup_and_its_handles_stay_harmless() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(&mut g, &mut w, "(finally! (wait) (note 1))"));
    let q = parked(run(&mut g, &mut w, "(finally! (wait) (note 2))"));
    drop(g);
    assert!(w.log.is_empty());
    let mut other = grift();
    let refused = other.resume(&mut w, p, Ok(Value::Inert)).unwrap_err();
    assert_eq!(kind(&other, &refused.error), "foreign-value");
    drop((refused, q));
    assert!(other.forgotten().is_none());
}

#[test]
fn another_interpreters_handle_is_refused_and_comes_back_whole() {
    let (mut g, mut h, mut w) = (grift(), grift(), World::default());
    let p = parked(run(&mut g, &mut w, "(add 1 (wait))"));
    let refused = h.resume(&mut w, p, Ok(Value::Integer(1))).unwrap_err();
    assert_eq!(kind(&h, &refused.error), "foreign-value");
    let refused = give_up(&mut h, &mut w, refused.parked.unwrap()).unwrap_err();
    assert_eq!(kind(&h, &refused.error), "foreign-value");
    // An interpreter put where the old one was is another interpreter.
    let old = std::mem::replace(&mut g, grift());
    let refused = g
        .resume(&mut w, refused.parked.unwrap(), Ok(Value::Integer(1)))
        .unwrap_err();
    assert_eq!(kind(&g, &refused.error), "foreign-value");
    g = old;
    let done = resume(&mut g, &mut w, refused.parked.unwrap(), Value::Integer(1));
    assert_eq!(finished(&g, done), "2");
}

#[test]
fn values_across_a_wait() {
    let (mut g, mut w) = (grift(), World::default());
    // A binding changed meanwhile is seen after; a local held by the run stays.
    g.evaluate(&mut w, "(define! shared 1)").unwrap();
    let p = parked(run(
        &mut g,
        &mut w,
        "(let! ((before shared) (mine (list 1 2))) (wait) (list before shared mine))",
    ));
    g.evaluate(&mut w, "(define! shared 2)").unwrap();
    collect(&mut g, &mut w);
    let done = resume(&mut g, &mut w, p, Value::Inert);
    assert_eq!(finished(&g, done), "(1 2 (1 2))");

    // The waiting call's arguments are the host's to keep: the machine lets them go at the park.
    let p = parked(run(&mut g, &mut w, "(wait-with (list 1 2 3))"));
    let argument = w.saved.pop().unwrap();
    let kept = g.keep(argument.clone()).unwrap();
    let q = parked(run(&mut g, &mut w, "(wait-with (list 4 5 6))"));
    let unkept = w.saved.pop().unwrap();
    collect(&mut g, &mut w);
    assert_eq!(g.list_items(&argument).unwrap().len(), 3);
    assert!(kind(&g, &g.list_items(&unkept).unwrap_err()) == "stale-value");

    // A stale answer is that call's error, as a host function's return is, and it can be caught.
    let done = resume(&mut g, &mut w, q, unkept);
    assert_eq!(kind(&g, &failed(done)), "stale-value");
    let answer = g.kept(&kept).unwrap();
    let done = resume(&mut g, &mut w, p, answer);
    assert_eq!(finished(&g, done), "(1 2 3)");
    g.release(kept).map_err(|(e, _)| e).unwrap();
    // Another interpreter's value is refused the same way. The table's freed entries go to a
    // value and a run alike, and each keeps its own.
    let mut other = grift();
    let foreign = other.string("x").unwrap();
    let k = g.keep(Value::Integer(7)).unwrap();
    let p = parked(run(
        &mut g,
        &mut w,
        "(catch! (wait) e (evaluate-in! kind e))",
    ));
    assert_eq!((g.kept(&k).unwrap(), g.parked()), (Value::Integer(7), 1));
    let done = resume(&mut g, &mut w, p, foreign);
    assert_eq!(finished(&g, done), "foreign-value");
}

#[test]
fn a_stop_during_the_call_that_would_park_stops_the_run_instead() {
    let (mut g, mut w) = (grift(), World::default());
    w.stopper = Some(g.interrupter());
    let e = failed(run(&mut g, &mut w, "(finally! (wait-with) (note 1))"));
    assert_eq!(kind(&g, &e), "interrupted");
    assert_eq!(w.log, ["1"]);
    assert_eq!(g.parked(), 0);
}

#[test]
fn a_stop_belongs_to_the_run_in_progress_not_to_those_parked() {
    let (mut g, mut w) = (grift(), World::default());
    let other = parked(run(&mut g, &mut w, "(finally! (add 1 (wait)) (note 1))"));
    let p = parked(run(
        &mut g,
        &mut w,
        "(define! spin (lambda! () (spin))) (finally! (sequence! (wait) (spin)) (note 2))",
    ));
    let mut stopper = g.interrupter();
    let watchdog = std::thread::spawn(move || {
        while !stopper.stop() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    let done = resume(&mut g, &mut w, p, Value::Inert);
    watchdog.join().unwrap();
    assert_eq!(kind(&g, &failed(done)), "interrupted");
    assert_eq!(w.log, ["2"]);
    let done = resume(&mut g, &mut w, other, Value::Integer(1));
    assert_eq!(finished(&g, done), "2");
    assert_eq!(w.log, ["2", "1"]);
}

#[test]
fn a_cleanup_may_park_while_a_stop_waits_and_the_stop_goes_on_after() {
    let (mut g, mut w) = (grift(), World::default());
    w.stopper = Some(g.interrupter());
    let p = parked(run(
        &mut g,
        &mut w,
        "(finally! (finally! (wait-with) (sequence! (wait) (note 1))) (note 2))",
    ));
    w.stopper = None;
    collect(&mut g, &mut w);
    let done = resume(&mut g, &mut w, p, Value::Inert);
    assert_eq!(kind(&g, &failed(done)), "interrupted");
    assert_eq!(w.log, ["1", "2"]);
}

#[test]
fn a_deep_run_parks_and_comes_back_whole() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(
        &mut g,
        &mut w,
        "(define! down (lambda! (n) (if! (same? n 0) (wait) (add 1 (down (subtract n 1))))))
         (down 200000)",
    ));
    collect(&mut g, &mut w);
    let done = resume(&mut g, &mut w, p, Value::Integer(1));
    assert_eq!(finished(&g, done), "200001");
}

/// Runs `src` with `wait` parking and again with the same call answering at once, and compares
/// everything a program or its host can see.
fn same_as_answered_at_once(src: &str) {
    let (mut g, mut w) = (grift(), World::default());
    let mut ran = run(&mut g, &mut w, src);
    while let Ok(Ran::Parked(p)) = ran {
        ran = resume(&mut g, &mut w, p, Value::Integer(9));
    }
    let (mut h, mut v) = (grift(), World::default());
    v.saved = vec![Value::Integer(9); 8];
    let answered = src.replace("(wait)", "(answer)");
    let plain = run(&mut h, &mut v, &answered);
    assert_eq!(w.log, v.log, "{src}");
    match (ran, plain) {
        (Ok(Ran::Finished(a)), Ok(Ran::Finished(b))) => {
            let (mut x, mut y) = (String::new(), String::new());
            g.write(&a, &mut x).unwrap();
            h.write(&b, &mut y).unwrap();
            assert_eq!(x, y, "{src}");
        }
        (Err(a), Err(b)) => {
            let text = described(&g, &a);
            assert_eq!(
                text,
                described(&h, &b).replace("(answer)", "(wait)"),
                "{src}"
            );
        }
        (a, b) => panic!("{src}: {a:?} and {b:?}"),
    }
}

#[test]
fn a_program_cannot_tell_a_wait_from_any_call() {
    for src in [
        "(map (lambda! (x) (add x (wait))) (list 1 2 3))",
        "(for-each (lambda! (x) (note (add x (wait)))) (list 1 2 3))",
        "(let! ((a (wait)) (b (add 1 (wait)))) (note a) (list a b))",
        "(finally! (note (wait)) (note (wait)))",
        "(catch! (first (wait)) e (evaluate-in! kind e))",
        "(catch! (finally! (first (wait)) (note (wait))) e (list (evaluate-in! kind e) (wait)))",
        "(if! (same? (wait) 9) (wait) 0)",
        "(sequence! (wait) (note 1) (wait))",
        "(conditional! ((same? (wait) 0) 1) (#true (wait)))",
        "(when! (same? (wait) 9) (note 1) (wait))",
        "(define! f (lambda! (x)\n  (first x)))\n(define! g (lambda! ()\n  (add 1 (f (wait)))))\n(g)",
        "(finally! (first (wait))\n  (sequence! (wait) (note 1)))",
        "(finally!\n  (add 1 (finally! (first 1) (wait)))\n  (note 2))",
        "(if! (wait) 1 2)",
        "(sequence! (wait) (first (wait)))",
    ] {
        same_as_answered_at_once(src);
    }
}

#[test]
fn a_trace_after_a_resume_shows_the_whole_path_through_a_load() {
    let (mut g, mut w) = (grift(), World::default());
    // The runner's load: forms read one by one, in a cleanup's scope, in the caller's context.
    g.evaluate(
        &mut w,
        "(define! load (wrap (vau! (path) caller
           (finally! (for-each (lambda! (form) (evaluate form caller)) (read-forms-host path))
                     (note (quote! closed))))))",
    )
    .unwrap();
    let p = parked(run(&mut g, &mut w, "\n(load \"inner.grift\")"));
    assert_eq!(w.log, ["loaded"]);
    collect(&mut g, &mut w);
    let e = failed(resume(&mut g, &mut w, p, Value::Integer(1)));
    assert_eq!(w.log, ["loaded", "closed"]);
    assert_eq!(
        described(&g, &e),
        "type-error at inner.grift:3 1 in (first from-file)
  from (for-each (lambda! (form) (evaluate form caller)) (read-forms-host path))
  from (finally! ... (note (quote! closed)))
  from (load \"inner.grift\") at script.grift:2"
    );
}

#[test]
fn a_trace_keeps_a_built_combination_only_the_parked_run_still_holds() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(
        &mut g,
        &mut w,
        "(evaluate (list (quote! wait)) (current-context))",
    ));
    collect(&mut g, &mut w);
    let e = g.error("timed-out", &[]);
    let done = went_on(g.resume(&mut w, p, Err(e)));
    assert!(described(&g, &failed(done)).starts_with("timed-out at script.grift:1 in (wait)"));
}

/// A run begun inside a host call is refused, by name, and the run that made the call can still
/// park.
#[test]
fn a_run_inside_a_host_call_is_refused_by_name_and_the_outer_one_still_parks() {
    let (mut g, mut w) = (grift(), World::default());
    let e = failed(run(&mut g, &mut w, "(nest \"(wait)\")"));
    assert_eq!(
        described(&g, &e),
        "cannot-nest at script.grift:1 in (nest \"(wait)\")"
    );
    let p = parked(run(&mut g, &mut w, "(catch! (nest \"(wait)\") e (wait))"));
    let done = resume(&mut g, &mut w, p, Value::Integer(5));
    assert_eq!(finished(&g, done), "5");
}

#[test]
fn a_run_that_cannot_park_says_so() {
    let (mut g, mut w) = (grift(), World::default());
    let e = g.evaluate(&mut w, "(wait)").unwrap_err();
    assert_eq!(described(&g, &e), "cannot-park in (wait)");
    let f = g.evaluate(&mut w, "(lambda! () (wait))").unwrap();
    let e = g.call(&mut w, &f, &[]).unwrap_err();
    assert_eq!(kind(&g, &e), "cannot-park");
    assert!(!g.can_park());
}

#[test]
fn inside_a_host_call_nothing_starts_or_resumes() {
    let (mut g, mut w) = (grift(), World::default());
    w.held = Some(parked(run(&mut g, &mut w, "(add 1 (wait))")));
    let done = run(&mut g, &mut w, "(meddle) (wait) 7");
    assert_eq!(w.refused, ["cannot-nest", "cannot-nest", "cannot-nest"]);
    let done = resume(&mut g, &mut w, parked(done), Value::Inert);
    assert_eq!(finished(&g, done), "7");
    let p = w.held.take().unwrap();
    let done = resume(&mut g, &mut w, p, Value::Integer(1));
    assert_eq!(finished(&g, done), "2");
}

#[test]
fn a_host_panic_ends_its_own_run_and_leaves_the_parked_ones() {
    let (mut g, mut w) = (grift(), World::default());
    let other = parked(run(&mut g, &mut w, "(add 1 (wait))"));
    let p = parked(run(&mut g, &mut w, "(wait) (boom)"));
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = g.resume(&mut w, p, Ok(Value::Inert));
    }));
    assert!(caught.is_err());
    assert_eq!(g.evaluate(&mut w, "(add 1 1)").unwrap(), Value::Integer(2));
    let e = g.evaluate(&mut w, "(wait)").unwrap_err();
    assert_eq!(kind(&g, &e), "cannot-park");
    let done = resume(&mut g, &mut w, other, Value::Integer(1));
    assert_eq!(finished(&g, done), "2");
}

#[test]
fn the_request_stays_good_for_as_long_as_the_run_waits() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(&mut g, &mut w, "(ask (list \"host\" 80))"));
    collect(&mut g, &mut w);
    let text = |g: &G, p: &Parked| {
        let mut s = String::new();
        g.write(p.request(), &mut s).unwrap();
        s
    };
    assert_eq!(text(&g, &p), "(\"host\" 80)");
    // A handle found again asks the same.
    drop(p);
    let p = g.forgotten().unwrap();
    collect(&mut g, &mut w);
    assert_eq!(text(&g, &p), "(\"host\" 80)");
    let done = resume(&mut g, &mut w, p, Value::Integer(1));
    assert_eq!(finished(&g, done), "1");
    // Once the run has gone on it holds its request no longer.
    let p = parked(run(&mut g, &mut w, "(ask (list 1))"));
    let asked = p.request().clone();
    let done = resume(&mut g, &mut w, p, Value::Inert);
    assert_eq!(finished(&g, done), "#inert");
    collect(&mut g, &mut w);
    assert_eq!(kind(&g, &g.list_items(&asked).unwrap_err()), "stale-value");
    // A request that isn't this interpreter's is that call's error: nothing parks.
    let mut other = grift();
    w.saved = vec![other.string("elsewhere").unwrap()];
    let e = failed(run(&mut g, &mut w, "(finally! (ask-saved) (note 1))"));
    assert_eq!(kind(&g, &e), "foreign-value");
    assert_eq!(w.log, ["1"]);
    assert_eq!(g.parked(), 0);
}

#[test]
fn refused_handles_survive_the_question_mark() {
    fn refused(g: &mut G, h: &mut G, w: &mut World) -> Result<Ran, Error> {
        let p = parked(run(g, w, "(finally! (wait) (note 1))"));
        Ok(h.resume(w, p, Ok(Value::Inert))?)
    }
    let (mut g, mut h, mut w) = (grift(), grift(), World::default());
    let e = refused(&mut g, &mut h, &mut w).unwrap_err();
    assert_eq!(kind(&h, &e), "foreign-value");
    assert_eq!(g.parked(), 1);
    let p = g.forgotten().unwrap();
    let ended = give_up(&mut g, &mut w, p).unwrap();
    assert_eq!(kind(&g, &ended), "given-up");
    assert_eq!(w.log, ["1"]);
}

#[test]
fn an_error_after_giving_up_shows_where_the_run_was_waiting() {
    let (mut g, mut w) = (grift(), World::default());
    let p = parked(run(&mut g, &mut w, "(add 1\n  (wait))"));
    let e = give_up(&mut g, &mut w, p).unwrap();
    assert_eq!(
        described(&g, &e),
        "given-up at script.grift:2 in (wait)\n  from (add 1 ...) at script.grift:1"
    );
}

/// A call the machine makes for a host function is part of the run, in a resumed run too: a wait
/// in it parks the run, where a run inside the host call could not have.
#[test]
fn a_call_made_for_a_host_function_in_a_resumed_run_parks_it() {
    let (mut g, mut w) = (grift(), World::default());
    let src = "(wait) (note (add 1 (on-machine (lambda! () (wait))))) (wait)";
    let p = parked(run(&mut g, &mut w, src));
    let p = parked(resume(&mut g, &mut w, p, Value::Inert));
    let p = parked(resume(&mut g, &mut w, p, Value::Integer(41)));
    assert_eq!(w.log, ["42"]);
    let done = resume(&mut g, &mut w, p, Value::Integer(5));
    assert_eq!(finished(&g, done), "5");
}

/// `(on-machine thunk)`: calls `thunk` on the machine, as this call's answer.
fn on_machine(_: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Call {
        function: args[0].clone(),
        arguments: Vec::new(),
        then: None,
    })
}

/// `(and-then f x)`: calls `f` on the machine, then `(note x result)`, and gives what `note` does.
fn and_then(g: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
    let top = g.top_level();
    let name = g.symbol("note")?;
    Ok(Answer::Call {
        function: args[0].clone(),
        arguments: Vec::new(),
        then: Some((g.lookup(&top, &name)?.unwrap(), vec![args[1].clone()])),
    })
}

#[test]
fn a_then_gets_its_saved_values_and_the_result_and_an_error_passes_it_by() {
    let (mut g, mut w) = (grift(), World::default());
    let v = g.evaluate(&mut w, "(and-then (lambda! () 41) 1)").unwrap();
    assert_eq!(v, Value::Inert);
    assert_eq!(w.log, ["1", "41"]);
    w.log.clear();
    let src = "(catch! (and-then (lambda! () (raise (error (quote! no)))) 1) e 7)";
    assert_eq!(g.evaluate(&mut w, src).unwrap(), Value::Integer(7));
    assert!(w.log.is_empty(), "then ran: {:?}", w.log);
}

/// `(each-step f items)`: calls `f` on each item with no Rust stack: its `then` is itself, and
/// what it must remember is in the saved values.
fn each_step(g: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
    let Some((x, rest)) = g.as_pair(&args[1])? else {
        return Ok(Answer::Now(Value::Inert));
    };
    let (top, name) = (g.top_level(), g.symbol("each-step")?);
    Ok(Answer::Call {
        function: args[0].clone(),
        arguments: vec![x],
        then: Some((g.lookup(&top, &name)?.unwrap(), vec![args[0].clone(), rest])),
    })
}

#[test]
fn a_host_function_loops_over_callbacks_by_answering_call_from_its_then() {
    let (mut g, mut w) = (grift(), World::default());
    let f = g.host_function("each-step", each_step).unwrap();
    g.define("each-step", f).unwrap();
    // The `then` gets the last item's result too, as a third argument it ignores.
    let v = g.evaluate(&mut w, "(each-step note (list 1 2 3))").unwrap();
    assert_eq!(v, Value::Inert);
    assert_eq!(w.log, ["1", "2", "3"]);
}

/// What giving a run up promises doesn't rest on what `stop` means.
mod giving_up_and_the_stop_rule {
    use super::*;

    fn stop_until_heard(mut stopper: Interrupter) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            while !stopper.stop() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        })
    }

    /// A stop cuts the cleanup it lands in and the next one runs. The cleanup starts the watchdog
    /// itself once it has noted 1, since a stop may land even before a cleanup's first form.
    #[test]
    fn a_stop_cuts_one_cleanup_and_the_rest_run() {
        let (mut g, mut w) = (grift(), World::default());
        let watch = g
            .host_function("watch", |g, w, _| {
                w.watchdog = Some(stop_until_heard(g.interrupter()));
                Ok(Answer::Now(Value::Inert))
            })
            .unwrap();
        g.define("watch", watch).unwrap();
        let p = parked(run(
            &mut g,
            &mut w,
            "(define! spin (lambda! (n) (if! (same? n 0) #inert (spin (subtract n 1)))))
             (finally! (finally! (wait) (sequence! (note 1) (watch) (spin 30000000) (note 2))) (note 3))",
        ));
        let e = give_up(&mut g, &mut w, p).unwrap();
        w.watchdog.take().unwrap().join().unwrap();
        assert_eq!(kind(&g, &e), "interrupted");
        assert_eq!(w.log, ["1", "3"]);
        assert_eq!(g.parked(), 0);
    }

    /// It needs no stop, and it reaches a cleanup that parked before it began.
    #[test]
    fn with_no_stop_at_all_it_ends_a_run_that_waits_inside_a_cleanup() {
        let (mut g, mut w) = (grift(), World::default());
        let p = parked(run(
            &mut g,
            &mut w,
            "(finally!
               (finally! (first 0) (sequence! (note 1) (wait) (note 2)))
               (note 3))",
        ));
        assert_eq!(w.log, ["1"]);
        let e = give_up(&mut g, &mut w, p).unwrap();
        // `given-up` replaced the type-error that was leaving, and cut the cleanup at its wait.
        assert_eq!(kind(&g, &e), "given-up");
        assert_eq!(w.log, ["1", "3"]);
        assert_eq!(g.parked(), 0);
    }

    /// A stop before it, with the run parked, finds nothing and cuts nothing of its cleanups.
    #[test]
    fn a_stop_before_it_cuts_nothing_of_its_cleanups() {
        let (mut g, mut w) = (grift(), World::default());
        let mut early = g.interrupter();
        let p = parked(run(
            &mut g,
            &mut w,
            "(finally! (wait) (sequence! (note 1) (note 2) (note 3)))",
        ));
        let mut stopper = g.interrupter();
        assert!(!early.stop(), "taken before it waited: it finds nothing");
        assert!(!stopper.stop(), "nothing is in progress: it finds nothing");
        let e = give_up(&mut g, &mut w, p).unwrap();
        assert_eq!(kind(&g, &e), "given-up");
        assert_eq!(w.log, ["1", "2", "3"]);
    }

    /// A cleanup's host function panics: the machine goes, and the handle is spent, so nothing is
    /// left to resume, and other parked runs don't notice.
    #[test]
    fn if_a_cleanup_is_ended_hard_the_run_is_gone_and_others_stay() {
        let (mut g, mut w) = (grift(), World::default());
        let other = parked(run(&mut g, &mut w, "(finally! (add 1 (wait)) (note 9))"));
        let p = parked(run(
            &mut g,
            &mut w,
            "(finally! (finally! (wait) (boom)) (note 1))",
        ));
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = give_up(&mut g, &mut w, p);
        }));
        assert!(caught.is_err());
        assert_eq!(w.log, Vec::<String>::new());
        assert_eq!(g.parked(), 1);
        assert!(g.forgotten().is_none());
        let done = resume(&mut g, &mut w, other, Value::Integer(1));
        assert_eq!(finished(&g, done), "2");
        assert_eq!(w.log, ["9"]);
    }
}

/// The moment a run parks, against a stop on another thread: `stop` says `true` only if the run
/// was in progress, and then it must not simply go on later as if nothing was asked. This run has
/// no cleanup that could park after the stop acted, so a run that came back parked was not the
/// one a `true` was for.
#[test]
fn a_stop_that_says_true_is_never_lost_as_its_run_parks() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering::SeqCst};

    #[derive(Default)]
    struct Signals {
        go: AtomicU64,
        said: AtomicU64,
    }
    static SIGNALS: Signals = Signals {
        go: AtomicU64::new(0),
        said: AtomicU64::new(0),
    };

    fn arm(_: &mut G, _: &mut World, _: &[Value]) -> Result<Answer, Error> {
        SIGNALS.go.fetch_add(1, SeqCst);
        Ok(Answer::Later(Value::Inert))
    }

    let (mut g, mut w) = (grift(), World::default());
    let f = g.host_function("arm", arm).unwrap();
    g.define("arm", f).unwrap();
    let body = g.evaluate(&mut w, "(lambda! () (arm))").unwrap();
    let body = g.keep(body).unwrap();
    let done = Arc::new(AtomicU64::new(0));
    let mut stopper = g.interrupter();
    let quit = Arc::clone(&done);
    let stopping = std::thread::spawn(move || {
        let mut seen = 0;
        let mut delay = 1u64;
        while quit.load(SeqCst) == 0 {
            let go = SIGNALS.go.load(SeqCst);
            if go == seen {
                std::thread::yield_now();
                continue;
            }
            seen = go;
            // A delay that sweeps across the moment of parking.
            delay = (delay
                .wrapping_mul(2862933555777941757)
                .wrapping_add(3037000493))
                >> 54;
            for _ in 0..delay {
                std::hint::spin_loop();
            }
            let said = u64::from(stopper.stop());
            SIGNALS.said.store(go << 1 | said, SeqCst);
        }
    });
    let (mut parks, mut stops) = (0, 0);
    for _ in 0..20_000 {
        let f = g.kept(&body).unwrap();
        let ran = g.start(&mut w, &f, &[]);
        let go = SIGNALS.go.load(SeqCst);
        while SIGNALS.said.load(SeqCst) >> 1 != go {
            std::thread::yield_now();
        }
        let said_true = SIGNALS.said.load(SeqCst) & 1 == 1;
        match ran {
            Ok(Ran::Parked(p)) => {
                parks += 1;
                assert!(!said_true, "a stop said true and its run parked anyway");
                let _ = g.resume(&mut w, p, Ok(Value::Inert)).unwrap();
            }
            Err(e) => {
                stops += 1;
                assert_eq!(kind(&g, &e), "interrupted");
            }
            Ok(Ran::Finished(v)) => panic!("{v:?}"),
        }
    }
    done.store(1, SeqCst);
    stopping.join().unwrap();
    assert_eq!(parks + stops, 20_000);
}

/// The runner's own `load`, unchanged: a form of the loaded file waits, and the file's later forms
/// and the cleanup around the load run after it.
#[test]
fn a_wait_inside_the_runners_load_goes_on_with_the_rest_of_the_file() {
    fn read_file(g: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
        let path = g.as_str(&args[0])?.unwrap().to_owned();
        g.string(&std::fs::read_to_string(&path).unwrap())
            .map(Answer::Now)
    }
    fn read_forms(g: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
        let (source, text) = (
            g.as_str(&args[0])?.unwrap().to_owned(),
            g.as_str(&args[1])?.unwrap().to_owned(),
        );
        g.read_in(&source, &text).map(Answer::Now)
    }
    fn unused(g: &mut G, _: &mut World, _: &[Value]) -> Result<Answer, Error> {
        Err(g.error("unused", &[]))
    }
    let (mut g, mut w) = (grift(), World::default());
    let private = g.private_context().unwrap();
    let hosts: [(&str, grift::HostFunction<World>); 9] = [
        ("read-file", read_file),
        ("read-forms", read_forms),
        ("display", unused),
        ("display-error", unused),
        ("position", unused),
        ("environment-variable", unused),
        ("wall-clock-nanoseconds", unused),
        ("elapsed-nanoseconds", unused),
        ("exit", unused),
    ];
    let mut bound = vec![
        ("top", g.top_level()),
        ("script-path", g.string("").unwrap()),
        ("command-line-arguments", Value::EmptyList),
    ];
    for (name, f) in hosts {
        bound.push((name, g.host_function(name, f).unwrap()));
    }
    for (name, value) in bound {
        let name = Value::Symbol(g.symbol(name).unwrap());
        g.bind(&private, &name, value).unwrap();
    }
    g.evaluate_in(
        &mut w,
        &private,
        "runner.grift",
        include_str!("../../grift-cli/src/runner.grift"),
    )
    .unwrap();
    let path = format!(
        "{}/tests/fixtures/waiting-load.grift",
        env!("CARGO_MANIFEST_DIR")
    );
    let p = parked(run(
        &mut g,
        &mut w,
        &format!("(finally! (load \"{path}\") (note 99))"),
    ));
    collect(&mut g, &mut w);
    let done = resume(&mut g, &mut w, p, Value::Integer(17));
    assert_eq!(finished(&g, done), "#inert");
    assert_eq!(w.log, ["17", "99"]);
    assert_eq!(g.evaluate(&mut w, "loaded").unwrap(), Value::Integer(17));
}

/// `(call-then f then)`: calls `f` on the machine, then `then` with its value.
fn call_then(_: &mut G, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Call {
        function: args[0].clone(),
        arguments: Vec::new(),
        then: Some((args[1].clone(), Vec::new())),
    })
}

/// A call and its `then` are each called as `Grift::call` calls, in a new empty context of its
/// own: what the call binds in its caller's context the `then` doesn't see.
#[test]
fn a_call_and_its_then_each_get_a_context_of_their_own() {
    let (mut g, mut w) = (grift(), World::default());
    let f = g.host_function("call-then", call_then).unwrap();
    g.define("call-then", f).unwrap();
    let src = "(call-then (wrap (vau! () e (bind e (quote! x) 1)))
                          (wrap (vau! (v) e (lookup e (quote! x) #false))))";
    assert_eq!(g.evaluate(&mut w, src).unwrap(), Value::Boolean(false));
}
