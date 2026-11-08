//! How a host ends a run: `cargo run -p grift --example stops`. A stop runs the cleanups, a force
//! cuts them, `is_limit` tells what the host began from what a program named the same, and a parked
//! run is given up by answering its wait with a limit.

use grift::{Answer, Grift, Interrupter, Ran, ResumeError, Value};
use std::thread;
use std::time::Duration;

const PROGRAM: &str = "
    (define! here (current-context))
    (define! closed 0)
    (define! close (lambda! () (bind here (quote! closed) (add closed 1))))
    (define! spin (lambda! () (spin)))";

/// What the host functions see: whether the watchdog forces.
struct Host {
    force: bool,
}

/// `(watch)`: from here on, another thread is watching this run, and has run out of patience. A
/// real watchdog starts with its run; this one starts inside it, so that the stop can't come before
/// the program's cleanups are in place. Its handle is for this run and no other, so it can't stop
/// the next even late. Nothing joins it: it ends by itself.
fn watch(g: &mut Grift<Host>, host: &mut Host, _: &[Value]) -> Result<Value, grift::Error> {
    let mut handle: Interrupter = g.interrupter();
    let force = host.force;
    thread::spawn(move || {
        handle.stop();
        if force {
            thread::sleep(Duration::from_millis(20));
            handle.force();
        }
    });
    Ok(Value::Inert)
}

pub fn main() -> Result<(), grift::Error> {
    let mut g: Grift<Host> = Grift::new()?;
    let mut host = Host { force: false };
    let f = g.host_function("watch", watch)?;
    g.define("watch", f)?;
    let wait = g.parking_function("wait", |_, _, _| Ok(Answer::Later(Value::Inert)))?;
    g.define("wait", wait)?;
    g.evaluate(&mut host, PROGRAM)?;

    // A stop passes the `catch!` and runs the cleanup on its way out.
    let src = "(catch! (finally! (sequence! (watch) (spin)) (close)) #ignore 0)";
    let ended = g.evaluate(&mut host, src);
    assert!(
        ended.is_err_and(
            |e| e.is_limit() && g.symbol_name(&e.kind).is_ok_and(|k| k == "interrupted")
        )
    );
    assert_eq!(g.evaluate(&mut host, "closed")?, Value::Integer(1));
    println!("stop: interrupted, and the cleanup ran");

    // A cleanup that never ends outlasts a stop. A force ends it, and whatever is around it.
    host.force = true;
    let src = "(finally! (finally! (sequence! (watch) (spin)) (spin)) (close))";
    let ended = g.evaluate(&mut host, src);
    assert!(ended.is_err_and(|e| e.is_limit()));
    assert_eq!(g.evaluate(&mut host, "closed")?, Value::Integer(1));
    println!("force: interrupted, and no cleanup past it ran");

    // A name makes nothing real: a program's own `interrupted` is data, and a `catch!` catches it.
    let forged = g.evaluate(&mut host, "(raise (error (quote! interrupted)))");
    assert!(
        forged.is_err_and(
            |e| g.symbol_name(&e.kind).is_ok_and(|k| k == "interrupted") && !e.is_limit()
        )
    );
    println!("is_limit: a program's own interrupted is not one");

    // A run that waits holds its cleanups. Other runs go on; a limit as the answer ends it.
    let top = g.top_level();
    let src = "(catch! (finally! (wait) (close)) #ignore 0)";
    let Ran::Parked(parked) = g.start_in(&mut host, &top, "script", src)? else {
        return Err(g.error("it-did-not-wait", &[]));
    };
    assert_eq!(g.parked(), 1);
    assert_eq!(g.evaluate(&mut host, "(add 2 2)")?, Value::Integer(4));
    let ending = g.error("given-up", &[]).as_limit();
    let Err(ResumeError {
        error,
        parked: None,
    }) = g.resume(&mut host, parked, Err(ending))
    else {
        return Err(g.error("it-went-on", &[]));
    };
    assert!(error.is_limit() && g.symbol_name(&error.kind)? == "given-up");
    assert_eq!(g.evaluate(&mut host, "closed")?, Value::Integer(2));
    assert_eq!(g.parked(), 0);
    println!("parked: given up with a limit, and its cleanup ran");
    Ok(())
}
