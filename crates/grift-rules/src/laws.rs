//! The laws grift keeps, each checked on generated programs: two fresh interpreters agree; a handler that raises again, a cleanup, and library forms
//! shadowed by plain operatives change nothing; a handler catches every error a program raises;
//! collecting at every safe point while parked at each wait changes nothing; runs parked together
//! mean what each means alone, up to a stop or force asked inside one; memory refused at any
//! allocation ends the run as out of memory or as it would have ended; printed data reads back as
//! itself. After anything that went wrong the interpreter means what it meant. A failure prints
//! the seed and the smallest program that still fails; `GRIFT_LAW_SEED` and `GRIFT_LAW_CASES`
//! choose other and more seeds.
//!
//! What a program noted is kept in the language itself (`note` conses onto a top-level list), so
//! no core needs host state for it; `stop-here` and `force-here` note a mark and then call the
//! [`Host::Stop`](crate::Host::Stop) and [`Host::Force`](crate::Host::Force) host functions.

use crate::generate::{self, Node, Options, Rng};
use crate::{Core, Outcome, fresh, outcome};
use grift::{Error, Parked, Ran, Value};

/// What a run came to and what it noted on the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub outcome: Outcome,
    /// How many places an error's trace has.
    pub trace: Option<usize>,
    pub log: Vec<String>,
}

impl Seen {
    /// The same but for the trace, which a wrapper adds its own places to.
    fn same_meaning(&self, other: &Seen) -> bool {
        self.outcome == other.outcome && self.log == other.log
    }
}

fn differ(what: &str, a: &Seen, b: &Seen) -> String {
    format!("{what}:\n  alone   {a:?}\n  changed {b:?}")
}

/// The definitions a run with log `id` uses: `note`, `cleanup-note`, `stop-here` and
/// `force-here`, suffixed by `id` when it isn't empty. With `interrupts` false, `stop-here` and
/// `force-here` only note their mark.
fn definitions(id: &str, interrupts: bool) -> String {
    let (stop, force) = if interrupts {
        ("stop-run", "force-run")
    } else {
        ("echo", "echo")
    };
    format!(
        r#"(define! log{id}* ())
(define! note{id} (lambda! (x) (sequence! (bind top (quote! log{id}*) (pair (value-to-string x) log{id}*)) x)))
(define! cleanup-note{id} (lambda! (x) (sequence! (bind top (quote! log{id}*) (pair (join "cleanup: " (value-to-string x)) log{id}*)) x)))
(define! stop-here{id} (lambda! (x) (sequence! (note{id} (quote! stop-mark)) ({stop} x))))
(define! force-here{id} (lambda! (x) (sequence! (note{id} (quote! force-mark)) ({force} x))))
#inert"#
    )
}

/// `src` written to use the definitions of log `id`.
fn renamed(src: &str, id: &str) -> String {
    if id.is_empty() {
        return src.to_string();
    }
    ["note", "cleanup-note", "stop-here", "force-here"]
        .iter()
        .fold(src.to_string(), |s, n| {
            s.replace(&format!("({n} "), &format!("({n}{id} "))
        })
}

/// A fresh interpreter with the harness's host functions and the laws' definitions.
pub fn setup(interrupts: bool) -> Result<Core, String> {
    let mut c = fresh(None)?;
    define(&mut c, "", interrupts)?;
    Ok(c)
}

fn define(c: &mut Core, id: &str, interrupts: bool) -> Result<(), String> {
    let r = c.eval(&definitions(id, interrupts));
    match outcome(c, r) {
        Outcome::Value(_) => Ok(()),
        o => Err(format!("the laws' definitions: {o:?}")),
    }
}

/// What log `id` holds, oldest first, emptied.
fn take_log(c: &mut Core, id: &str) -> Vec<String> {
    let mut log = Vec::new();
    let next = format!(
        "(if! (same? log{id}* ()) #false (sequence! (define! entry* (first log{id}*)) (bind top (quote! log{id}*) (rest log{id}*)) entry*))"
    );
    while log.len() < 100_000 {
        let r = c.eval(&next);
        match outcome(c, r) {
            // Entries are strings: the log holds their text.
            Outcome::Value(s) if s != "#false" => {
                let text = s.strip_prefix('"').and_then(|t| t.strip_suffix('"'));
                log.push(text.map_or(s.clone(), str::to_string));
            }
            Outcome::Value(_) => break,
            o => {
                log.push(format!("<log unreadable: {o:?}>"));
                break;
            }
        }
    }
    log.reverse();
    log
}

/// What the `n`th wait of a run is answered with: every third an error `timed-out`, else 7.
fn answer(c: &mut Core, n: usize) -> Result<Value, &'static str> {
    if n % 3 == 2 {
        return Err("timed-out");
    }
    c.eval("7").map_err(|_| "no-answer")
}

/// The `Seen` of a run that ended with `r`.
fn ended(c: &mut Core, r: Result<Value, Error>, id: &str) -> Seen {
    let trace = r.as_ref().err().map(|e| c.trace_len(e));
    let outcome = outcome(c, r);
    Seen {
        outcome,
        trace,
        log: take_log(c, id),
    }
}

/// Runs `src` in a run that may park, answering each wait at once.
pub fn run(c: &mut Core, src: &str) -> Seen {
    run_refused(c, src, None)
}

/// `run`, with memory refused from the `refuse`th allocation of the run on. Waits are answered
/// with a value kept beforehand, so answering needs no memory.
fn run_refused(c: &mut Core, src: &str, refuse: Option<usize>) -> Seen {
    let Ok(seven) = c.eval("7").and_then(|v| c.keep(v)) else {
        return Seen {
            outcome: Outcome::OutOfMemory,
            trace: None,
            log: take_log(c, ""),
        };
    };
    c.refuse_allocations_from(refuse);
    let mut ran = c.start(src);
    let mut res = None;
    for n in 0..10_000 {
        match ran {
            Ok(Ran::Parked(p)) => {
                let a = if n % 3 == 2 {
                    Err("timed-out")
                } else {
                    c.kept(&seven).map_err(|_| "no-answer")
                };
                ran = c.resume(p, a);
            }
            Ok(Ran::Finished(v)) => {
                res = Some(Ok(v));
                break;
            }
            Err(e) => {
                res = Some(Err(e));
                break;
            }
        }
    }
    c.refuse_allocations_from(None);
    let _ = c.release(seven);
    match res {
        Some(r) => ended(c, r, ""),
        None => Seen {
            outcome: Outcome::Parked,
            trace: None,
            log: take_log(c, ""),
        },
    }
}

/// Whether `src` reads as data: a program that doesn't read can't be wrapped, so the wrapping
/// laws pass it by (the differential still compares it).
fn reads(src: &str) -> Result<bool, String> {
    let mut c = setup(false)?;
    let r = c.eval(&format!("(quote! {src})"));
    Ok(matches!(outcome(&mut c, r), Outcome::Value(_)))
}

/// `src` alone in a fresh interpreter.
fn alone(src: &str) -> Result<Seen, String> {
    setup(false).map(|mut c| run(&mut c, src))
}

fn env(k: &str) -> Option<u64> {
    std::env::var(k).ok().and_then(|v| v.parse().ok())
}

/// Runs `law` on `cases` slice programs.
pub fn check(
    name: &str,
    cases: u64,
    waits: bool,
    law: impl Fn(&Node, u64) -> Result<(), String>,
) -> Result<(), String> {
    let o = Options { waits };
    let first = env("GRIFT_LAW_SEED").unwrap_or(1);
    generate::check(name, first, env("GRIFT_LAW_CASES").unwrap_or(cases), o, law)
        .map_err(|e| format!("{e}\n(GRIFT_LAW_SEED=<seed> GRIFT_LAW_CASES=1 runs that seed alone)"))
}

/// The interpreter after something went wrong means what a fresh one does: a probe program, and
/// `src` again, give what they give alone.
fn usable(c: &mut Core, src: &str, seed: u64) -> Result<(), String> {
    let o = Options { waits: true };
    let probe = generate::program(seed.wrapping_add(1_000_003), o).text();
    for p in [probe.as_str(), src] {
        let (want, after) = (alone(p)?, run(c, p));
        if after != want {
            return Err(differ(&format!("afterwards, {p}"), &want, &after));
        }
    }
    Ok(())
}

/// Two fresh interpreters agree, and a handler that raises again changes neither the outcome
/// nor the notes, and keeps the trace's places.
#[test]
fn handler() -> Result<(), String> {
    check("handler", 300, false, |p, _| {
        let src = p.text();
        let (a, b) = (alone(&src)?, alone(&src)?);
        if a != b {
            return Err(differ("two fresh runs differ", &a, &b));
        }
        let wrapped = alone(&format!(
            "(evaluate-with-handler (quote! {src}) top (lambda! (e) (raise e)))"
        ))?;
        let traced = match (a.trace, wrapped.trace) {
            (Some(x), Some(y)) => y + 1 >= x,
            _ => true,
        };
        if !a.same_meaning(&wrapped) || !traced {
            return Err(differ("raised again", &a, &wrapped));
        }
        Ok(())
    })
}

/// A handler catches every error a program raises, whatever its kind is called (a generated
/// program meets no real limit), and the notes stay as they were.
#[test]
fn catch() -> Result<(), String> {
    check("catch", 300, false, |p, _| {
        let src = p.text();
        if !reads(&src)? {
            return Ok(());
        }
        let a = alone(&src)?;
        let caught = alone(&format!(
            "(evaluate-with-handler (quote! {src}) top (lambda! (e) (list (quote! caught) (lookup e (quote! kind) ()))))"
        ))?;
        let kind = match &a.outcome {
            Outcome::Error(k) => Some(k.as_str()),
            Outcome::OutOfMemory => Some("out-of-memory"),
            Outcome::Interrupted => Some("interrupted"),
            _ => None,
        };
        let ok = match kind {
            None => a.same_meaning(&caught),
            Some(k) => {
                caught.outcome == Outcome::Value(format!("(caught {k})")) && caught.log == a.log
            }
        };
        if !ok {
            return Err(differ("caught", &a, &caught));
        }
        Ok(())
    })
}

/// A cleanup runs once, after everything the program noted, and changes nothing else.
#[test]
fn cleanup() -> Result<(), String> {
    check("cleanup", 300, false, |p, _| {
        let src = p.text();
        if !reads(&src)? {
            return Ok(());
        }
        let mut a = alone(&src)?;
        let wrapped = alone(&format!(
            "(evaluate-with-cleanup (quote! {src}) top (lambda! () (note (quote! last-cleanup))))"
        ))?;
        a.log.push("last-cleanup".into());
        if !a.same_meaning(&wrapped) {
            return Err(differ("in a cleanup", &a, &wrapped));
        }
        Ok(())
    })
}

/// Nothing is special by name: with `if!`, `sequence!`, `let!` and `lambda!` shadowed by
/// operatives that only hand their operands on, a program means the same.
#[test]
fn shadowed() -> Result<(), String> {
    let prelude: String = ["if!", "sequence!", "let!", "lambda!"]
        .iter()
        .map(|n| format!("(bind shadowed (quote! {n}) (let! ((o {n})) (vau! a env (evaluate (pair o a) env))))"))
        .collect();
    check("shadowed", 300, false, |p, _| {
        let src = p.text();
        if !reads(&src)? {
            return Ok(());
        }
        let a = alone(&src)?;
        let s = alone(&format!(
            "(define! shadowed (context top)) {prelude} (evaluate (quote! {src}) shadowed)"
        ))?;
        if !a.same_meaning(&s) {
            return Err(differ("shadowed", &a, &s));
        }
        Ok(())
    })
}

/// Collecting at every safe point, parked at each wait, loses nothing a program can see.
#[test]
fn stress() -> Result<(), String> {
    check("stress", 150, true, |p, _| {
        let src = p.text();
        let a = alone(&src)?;
        let mut c = setup(false)?;
        c.collect_at_every_safe_point(true);
        let mut s = run(&mut c, &src);
        c.collect_at_every_safe_point(false);
        // The log is read after the run, outside the stress, but it was written under it.
        s.log.extend(take_log(&mut c, ""));
        if a != s {
            return Err(differ("collecting at every safe point", &a, &s));
        }
        Ok(())
    })
}

/// Memory refused from allocation N on, for N = 1, 2, ... up to what the run needs (at most 200
/// values of N, spread over it), ends the run as out of memory or as it would have ended; and
/// the interpreter means what it meant.
#[test]
fn memory() -> Result<(), String> {
    check("memory", 40, true, |p, seed| {
        let src = p.text();
        let a = alone(&src)?;
        let mut c = setup(false)?;
        let before = c.allocations();
        let _ = run(&mut c, &src);
        let needs = c.allocations().saturating_sub(before).max(1);
        let step = needs.div_ceil(200).max(1);
        for n in (1..=needs).step_by(step) {
            let r = run_refused(&mut c, &src, Some(n));
            let oom = matches!(
                r.outcome,
                Outcome::OutOfMemory | Outcome::Unprintable(crate::Ending::OutOfMemory)
            );
            if !oom && !a.same_meaning(&r) {
                return Err(differ(
                    &format!("memory refused from allocation {n}"),
                    &a,
                    &r,
                ));
            }
            usable(&mut c, &src, seed)?;
        }
        Ok(())
    })
}

/// What a program returns, printed, reads back as itself when it is data: no combiner or
/// context, and no symbol `string-to-symbol` made, which may not.
#[test]
fn read_back() -> Result<(), String> {
    check("read back", 300, false, |p, _| {
        let src = p.text();
        let mut c = setup(false)?;
        if let Outcome::Value(text) = run(&mut c, &src).outcome
            && !text.contains("#[")
            && !src.contains("string-to-symbol")
        {
            let r = c.eval(&format!("(quote! {text})"));
            let back = outcome(&mut c, r);
            if back != Outcome::Value(text.clone()) {
                return Err(format!("{text} reads back as {back:?}"));
            }
        }
        Ok(())
    })
}

/// How a run of several was touched.
#[derive(Default)]
struct Touched {
    /// Memory was (or may have been) refused while it ran.
    refused: bool,
    /// How long its log was when that came.
    clean_until: Option<usize>,
}

/// Up to four programs run together in one interpreter, parked at their waits and taken up in
/// random order, some with a stop or a force asked inside, some with memory refused while they
/// run, the collector at every safe point now and then. A run nothing touched means what it means
/// alone; one touched means that up to there, then ends with the limit that touched it, a stop
/// running only cleanups after it and a force nothing; and the interpreter means what it meant.
#[test]
fn together() -> Result<(), String> {
    let o = Options { waits: true };
    check("together", 200, true, |p, seed| {
        let mut r = Rng::new(seed ^ 0x7091);
        let mut programs = vec![p.clone()];
        for i in 1..1 + r.below(4) as u64 {
            programs.push(generate::program(
                seed.wrapping_mul(7919).wrapping_add(i),
                o,
            ));
        }
        for q in &mut programs {
            let ask = ["stop-here", "force-here", "", ""];
            let ask = ask.get(r.below(ask.len())).copied().unwrap_or("");
            if !ask.is_empty() {
                *q = q.wrapped_at(r.below(q.size()), &format!("({ask} {{}})"));
            }
        }
        let texts: Vec<String> = programs.iter().map(Node::text).collect();
        let alone: Vec<Seen> = texts.iter().map(|t| alone(t)).collect::<Result<_, _>>()?;
        let mut c = setup(true)?;
        let ids: Vec<String> = (0..texts.len()).map(|i| format!("-{i}")).collect();
        for id in &ids {
            define(&mut c, id, true)?;
        }
        let stress = r.below(4) == 0;
        c.collect_at_every_safe_point(stress);
        let n = texts.len();
        let mut touched: Vec<Touched> = (0..n).map(|_| Touched::default()).collect();
        let mut waits = vec![0; n];
        let mut waiting: Vec<(usize, Parked)> = Vec::new();
        let mut next = 0;
        for _ in 0..10_000 {
            if next >= n && waiting.is_empty() {
                break;
            }
            let (i, ran) = if next < n && (waiting.is_empty() || r.below(3) == 0) {
                next += 1;
                let i = next - 1;
                let src = renamed(
                    texts.get(i).map_or("", |s| s.as_str()),
                    ids.get(i).map_or("", |s| s.as_str()),
                );
                (i, c.start(&src))
            } else {
                let (i, parked) = waiting.swap_remove(r.below(waiting.len()));
                let w = waits.get(i).copied().unwrap_or(0);
                if let Some(w) = waits.get_mut(i) {
                    *w += 1;
                }
                let a = answer(&mut c, w);
                let refuse = (r.below(8) == 0).then(|| 1 + r.below(200));
                let before = c.allocations();
                c.refuse_allocations_from(refuse);
                let ran = c.resume(parked, a);
                c.refuse_allocations_from(None);
                if let (Some(k), Some(t)) = (refuse, touched.get_mut(i))
                    && c.allocations().saturating_sub(before) >= k
                {
                    t.refused = true;
                    // The log so far can't be read without ending the run: the whole log is
                    // compared up to the first refusal by counting what was noted before.
                    t.clean_until.get_or_insert(0);
                }
                (i, ran)
            };
            let res = match ran {
                Ok(Ran::Parked(parked)) => {
                    waiting.push((i, parked));
                    continue;
                }
                Ok(Ran::Finished(v)) => Ok(v),
                Err(e) => Err(e),
            };
            let id = ids.get(i).map_or("", |s| s.as_str());
            let got = ended(&mut c, res, id);
            let (Some(a), Some(t)) = (alone.get(i), touched.get(i)) else {
                continue;
            };
            together_ended(a, &got, t).map_err(|why| {
                format!(
                    "run {i} of {n}: {}\n{why}",
                    texts.get(i).map_or("", |s| s.as_str())
                )
            })?;
        }
        c.collect_at_every_safe_point(false);
        usable(&mut c, &p.text(), seed)
    })
}

/// Whether a run of several ended as it may, by what touched it.
fn together_ended(alone: &Seen, got: &Seen, t: &Touched) -> Result<(), String> {
    let mark = |m: &str| got.log.iter().position(|l| l == m);
    let (stop, force) = (mark("stop-mark"), mark("force-mark"));
    let Some(cut) = [stop, force, t.clean_until].into_iter().flatten().min() else {
        return if got == alone {
            Ok(())
        } else {
            Err(differ("untouched", alone, got))
        };
    };
    let fail = |what: &str| Err(differ(what, alone, got));
    if got.log.get(..cut) != alone.log.get(..cut) {
        return fail("before it was touched");
    }
    if t.refused && (got.outcome == Outcome::OutOfMemory || got.same_meaning(alone)) {
        return Ok(());
    }
    if let Some(at) = force {
        if got.outcome != Outcome::Interrupted || got.log.len() != at + 1 {
            return fail("forced");
        }
        return Ok(());
    }
    let Some(at) = stop else {
        return fail("refused memory, then neither out of memory nor as alone");
    };
    if got.outcome != Outcome::Interrupted {
        return fail("stopped, but not interrupted");
    }
    if got
        .log
        .iter()
        .skip(at + 1)
        .any(|l| !l.contains("cleanup") && !l.ends_with("-mark"))
    {
        return fail("ran more than cleanups after a stop");
    }
    // Every cleanup entered runs, but that a stop may cut the one it lands in.
    if cleanups_missed(&got.log) > 1 {
        return fail("cleanups entered never ran");
    }
    Ok(())
}

/// How many cleanups were entered and never ran, from a log alone; cleanups run innermost first.
fn cleanups_missed(log: &[String]) -> usize {
    let mut entered = Vec::new();
    let mut missed = 0;
    for line in log {
        if let Some(id) = line.strip_prefix("entered") {
            entered.push(id);
        } else if let Some(id) = line.strip_prefix("cleanup")
            && let Some(at) = entered.iter().rposition(|e| *e == id)
        {
            missed += entered.len() - at - 1;
            entered.truncate(at);
        }
    }
    missed + entered.len()
}

/// The generator writes every form it knows within 400 programs, so a law can't go quiet on one.
#[test]
fn generator_writes_every_form() -> Result<(), String> {
    let o = Options { waits: true };
    let texts: Vec<String> = (1..401)
        .map(|seed| generate::program(seed, o).text())
        .collect();
    match generate::forms(o)
        .into_iter()
        .find(|f| !texts.iter().any(|t| t.contains(f)))
    {
        Some(form) => Err(format!("no program has {form}")),
        None => Ok(()),
    }
}
