//! The rules harness: the four resource rules (R1 stop, R2 no Rust recursion, R3 every
//! allocation can fail, R4 bounded output) applied to every operation family by one loop each.

use crate::{Core, Outcome, check, ending_outcome, fresh, outcome, ran_outcome, run, timed};
use grift::{Interrupter, Ran};
use std::time::{Duration, Instant};

/// The longest a stop may take to land.
pub const LATENCY: Duration = Duration::from_millis(50);

/// What a family's work is.
#[derive(Clone, Copy, Debug)]
pub enum Work {
    /// Evaluate this program.
    Eval(&'static str),
    /// Evaluate this program, which fails, and describe its error with a large budget.
    Describe(&'static str),
    /// Start `(wait 0)` and resume it with 0 into this program, the run's rest: `(sequence!
    /// (wait 0) <program>)`.
    Park(&'static str),
}

/// An operation family: a setup that makes data of size `n`, and the work on it (`N` in either
/// is replaced by the size).
#[derive(Clone, Copy, Debug)]
pub struct Family {
    pub name: &'static str,
    /// Defines the data, `N` the size.
    pub setup: &'static str,
    pub work: Work,
    /// The size for long runs (stop landing) and for small ones (allocation refusal).
    pub long: usize,
    pub small: usize,
}

/// Every family of operations a stop, a refusal and a budget must reach.
pub const FAMILIES: &[Family] = &[
    Family {
        name: "machine steps",
        setup: "(define! n N)",
        work: Work::Eval("(spin n)"),
        long: 1 << 26,
        small: 30,
    },
    Family {
        name: "read",
        setup: r#"(define! text (join "(" (doubled "a " N) ")"))"#,
        work: Work::Eval("(first (rest (read-form text 0)))"),
        long: 22,
        small: 3,
    },
    Family {
        name: "print",
        setup: "(define! data (count-list N ()))",
        work: Work::Eval("(byte-length (value-to-string data))"),
        long: 1 << 21,
        small: 20,
    },
    Family {
        name: "search",
        setup: r#"(define! text (doubled "a" N))"#,
        work: Work::Eval(r#"(search text "ab" 0)"#),
        long: 27,
        small: 4,
    },
    Family {
        name: "parameter matching",
        setup: "(define! tree (nest-list N (quote! x))) (define! arg (nest-list N 1)) \
                (define! op (wrap (evaluate (list vau! (list tree) #ignore 0) (context ()))))",
        work: Work::Eval("(op arg)"),
        long: 1 << 21,
        small: 10,
    },
    Family {
        name: "reports",
        setup: "(define! data (count-list N ()))",
        work: Work::Describe("(raise (make-error (quote! big) (list data data)))"),
        long: 1 << 21,
        small: 10,
    },
    Family {
        name: "collection",
        setup: "(define! data (count-list N ()))",
        work: Work::Eval("(churn (multiply N 30))"),
        long: 1 << 21,
        small: 20,
    },
    Family {
        name: "host calls",
        setup: "(define! n N) (define! hloop (lambda! (n) (if! (same? n 0) 0 (call-back hloop (subtract n 1)))))",
        work: Work::Eval("(hloop n)"),
        long: 1 << 26,
        small: 10,
    },
    Family {
        name: "park/resume",
        setup: "(define! n N)",
        work: Work::Park("(spin n)"),
        long: 1 << 26,
        small: 10,
    },
];

fn setup(f: &Family, n: usize) -> Result<Core, String> {
    let mut c = fresh(None)?;
    let src = f.setup.replace('N', &n.to_string());
    match run(&mut c, &src) {
        Outcome::Value(_) => Ok(c),
        o => Err(format!("{}: setup at size {n}: {o:?}", f.name)),
    }
}

/// Runs the family's work and gives its outcome. `before` runs once the work's run is about to
/// begin (after any parking), with the stopper for it.
fn work(c: &mut Core, w: Work, n: usize, before: impl FnOnce(Interrupter)) -> Outcome {
    let size = n.to_string();
    match w {
        Work::Eval(src) => {
            before(c.stopper());
            run(c, &src.replace('N', &size))
        }
        Work::Describe(src) => match c.eval(&src.replace('N', &size)) {
            Ok(v) => outcome(c, Ok(v)),
            Err(e) if !matches!(c.ending(&e), crate::Ending::Error(_)) => ending_outcome(c, &e),
            Err(e) => {
                before(c.stopper());
                match c.describe(&e, usize::MAX / 2) {
                    Ok((s, _)) => Outcome::Value(format!("report of {} bytes", s.len())),
                    Err(e) => ending_outcome(c, &e),
                }
            }
        },
        Work::Park(src) => match c.start(&format!("(sequence! (wait 0) {src})")) {
            Ok(Ran::Parked(p)) => {
                let zero = match c.eval("0") {
                    Ok(z) => z,
                    Err(e) => return ending_outcome(c, &e),
                };
                before(c.stopper());
                let r = c.resume(p, Ok(zero));
                ran_outcome(c, r)
            }
            r => ran_outcome(c, r),
        },
    }
}

/// R1 for one family: a stop asked before the work begins, and one asked during it from another
/// thread, each ends the work with `Interrupted`, the second within [`LATENCY`] of the ask; the
/// interpreter is usable after. Every stop thread is joined before the next run, so a stopper
/// that isn't tied to one run can't reach a later one. Returns the landing latency, or the
/// failures.
pub fn stop_lands(f: &Family) -> Result<Duration, String> {
    let mut c = setup(f, f.long)?;
    let mut failures = Vec::new();
    // Before: a backup stop at 1 s keeps a run that ignored it from going on.
    let mut backup = None;
    let (got, took) = timed(|| {
        work(&mut c, f.work, f.long, |mut s| {
            s.stop();
            backup = Some(later(s, Duration::from_secs(1), false));
        })
    });
    let _ = backup.map(|b| b.join());
    if got != Outcome::Interrupted || took >= Duration::from_secs(1) {
        failures.push(format!(
            "{}: stop asked before the run: {got:?} after {took:?}",
            f.name
        ));
    }
    usable(&mut c, f.name)?;
    // How long the work runs unstopped, capped by a stop at 1 s.
    let mut backup = None;
    let (_, free) = timed(|| {
        work(&mut c, f.work, f.long, |s| {
            backup = Some(later(s, Duration::from_secs(1), false))
        })
    });
    let _ = backup.map(|b| b.join());
    let delay = (free / 3).min(Duration::from_millis(300));
    let asked = std::sync::Arc::new(std::sync::Mutex::new(None::<Instant>));
    let asked2 = asked.clone();
    let mut stopper = None;
    let got = work(&mut c, f.work, f.long, |mut s| {
        stopper = Some(std::thread::spawn(move || {
            std::thread::sleep(delay);
            if let Ok(mut a) = asked2.lock() {
                *a = Some(Instant::now());
            }
            s.stop();
        }));
    });
    let ended = Instant::now();
    let _ = stopper.map(|t| t.join());
    let at = asked.lock().ok().and_then(|a| *a);
    let latency = at.map_or(Duration::ZERO, |at| ended.saturating_duration_since(at));
    match got {
        Outcome::Interrupted => {}
        // Finished before the stop: the work is bounded by its own size, under the latency.
        _ if at.is_none() || free < LATENCY => {}
        o => failures.push(format!(
            "{}: stop during (asked after {delay:?}): {o:?}",
            f.name
        )),
    }
    if latency >= LATENCY {
        failures.push(format!("{}: stop landed after {latency:?}", f.name));
    }
    usable(&mut c, f.name)?;
    if failures.is_empty() {
        Ok(latency)
    } else {
        Err(failures.join("; "))
    }
}

/// Stops (or forces) the run `s` names after `delay`, from another thread.
pub fn later(mut s: Interrupter, delay: Duration, force: bool) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let _ = if force { s.force() } else { s.stop() };
    })
}

/// The interpreter still evaluates.
pub fn usable(c: &mut Core, what: &str) -> Result<(), String> {
    let o = run(c, "(add 1 2)");
    check(o == Outcome::Value("3".into()), || {
        format!("{what}: not usable after: {o:?}")
    })
}

/// R3 for one family: memory refused at the Nth allocation, for N = 1, 2, ... until the work
/// completes without reaching N: each run ends with its ordinary outcome or `OutOfMemory`, and
/// the interpreter is usable after. Returns how many N were tried.
pub fn refusals(f: &Family) -> Result<usize, String> {
    const CAP: usize = 20_000;
    let mut c = setup(f, f.small)?;
    let want = work(&mut c, f.work, f.small, |_| {});
    for n in 1..=CAP {
        let before = c.allocations();
        c.refuse_allocations_from(Some(n));
        let got = work(&mut c, f.work, f.small, |_| {});
        let used = c.allocations().saturating_sub(before);
        c.refuse_allocations_from(None);
        check(got == want || got == Outcome::OutOfMemory, || {
            format!(
                "{}: refused at allocation {n}: {got:?}, want {want:?} or out-of-memory",
                f.name
            )
        })?;
        usable(&mut c, &format!("{} refused at {n}", f.name))?;
        if used < n && got == want {
            return Ok(n);
        }
    }
    Err(format!("{}: still refused after {CAP} allocations", f.name))
}
