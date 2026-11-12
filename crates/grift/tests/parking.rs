//! Runs that park: a host function answers "not yet", the run is set aside, and the host's loop
//! goes on until it resumes it, with a value or an error.
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

use grift::{Answer, Error, Grift, HostCall, Parked, Ran, ResumeError, Value};

#[derive(Default)]
struct World {
    log: Vec<String>,
}

type G = Grift<World>;

/// Gives a parked run up, as a host does: the waiting call answers a limit, and so does each wait
/// of a cleanup on the way out. What the run ended with; refused, why, the handle back.
fn give_up(g: &mut G, w: &mut World, mut p: Parked) -> Result<Error, ResumeError> {
    loop {
        let ending = g.limit("given-up", &[]);
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

fn wait(_: &mut HostCall<World>, _: &mut World, _: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Later(Value::Inert))
}

fn note(g: &mut HostCall<World>, world: &mut World, args: &[Value]) -> Result<Answer, Error> {
    for v in args {
        let mut text = String::new();
        g.write(v, &mut text).unwrap();
        world.log.push(text);
    }
    Ok(Answer::Now(Value::Inert))
}

fn boom(_: &mut HostCall<World>, _: &mut World, _: &[Value]) -> Result<Answer, Error> {
    panic!("a host function panics");
}

fn grift() -> G {
    let mut g = G::new().unwrap();
    for (name, f) in [
        ("wait", wait as grift::HostFunction<World>),
        ("note", note),
        ("boom", boom),
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

fn described(g: &G, e: &Error) -> String {
    let mut s = String::new();
    g.describe(e, &mut s, 4096).unwrap();
    s
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
    // Nor with no run in progress, after a panic ended one that could.
    let boom = std::panic::AssertUnwindSafe(|| run(&mut g, &mut w, "(boom)"));
    assert!(std::panic::catch_unwind(boom).is_err());
    assert!(!g.can_park());
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

/// `(call-then f then)`: calls `f` on the machine, then `then` with its value.
fn call_then(_: &mut HostCall<World>, _: &mut World, args: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Call {
        function: args[0],
        arguments: Vec::new(),
        then: Some((args[1], Vec::new())),
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
