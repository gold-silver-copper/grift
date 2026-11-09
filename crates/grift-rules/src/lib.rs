//! The rules grift keeps, as properties checked on its public API (`RULES.md`): one witness per
//! obligation (rows L1-L6, H1-H8, R1-R4, F1-F4), the rules harness that drives every operation
//! family through a stop, refused memory, a small stack and a budget, and the laws on generated
//! programs. Outcomes are compared by kind (a value's printed form, an error's kind, interrupted,
//! out of memory), never by a report's text. Test code only: the crate is empty outside `cargo
//! test`.
//!
//! Memory limits, refused allocations and collecting at every safe point come from grift's `hooks`
//! feature, which nothing else turns on.
#![cfg(test)]

mod generate;
mod laws;
mod rules;
mod witnesses;

use grift::{
    Answer, Error, Grift, Interrupter, Kept, Parked, Ran, ReportStatus, TextBuffer, Value,
};
use std::fmt::{self, Write};
use std::time::{Duration, Instant};

/// One grift interpreter, driven the way an embedder would, with the witnesses' host functions.
pub struct Core(Grift<()>);

/// How a run ended when it didn't give a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ending {
    /// An ordinary error of this kind.
    Error(String),
    /// A stop or force from the host.
    Interrupted,
    /// Memory refused.
    OutOfMemory,
}

/// What a run came to, compared by kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A value, printed (cut at 64 KiB).
    Value(String),
    Error(String),
    Interrupted,
    OutOfMemory,
    Parked,
    /// Printing the value failed, with this ending.
    Unprintable(Ending),
}

/// A sink that refuses past `left` bytes, so printing keeps a budget.
struct Budget {
    text: TextBuffer,
    left: usize,
    cut: bool,
}

impl Write for Budget {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() > self.left {
            let end = (0..=self.left)
                .rev()
                .find(|&i| s.is_char_boundary(i))
                .unwrap_or(0);
            let _ = self.text.write_str(s.get(..end).unwrap_or(""));
            self.left = 0;
            self.cut = true;
            return Err(fmt::Error);
        }
        self.left -= s.len();
        self.text.write_str(s)
    }
}

fn nth(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Inert)
}

fn oom(g: &mut Grift<()>) -> Error {
    g.error("out-of-memory", &[])
}

/// The host functions the witnesses' programs call, each bound by an ordinary define.
const HOSTS: &[(&str, grift::HostFunction<()>)] = &[
    // `(echo x)` answers `x`.
    ("echo", |_, _, args| Ok(Answer::Now(nth(args, 0)))),
    // `(fail x ...)` answers a catchable error of kind `host-failure` with values `x ...`.
    ("fail", |g, _, args| Err(g.error("host-failure", args))),
    // `(call-back f x ...)` answers a call of `f` with `x ...` in tail position.
    ("call-back", |g, _, args| {
        let rest = args.get(1..).unwrap_or(&[]);
        let mut arguments = Vec::new();
        arguments.try_reserve(rest.len()).map_err(|_| oom(g))?;
        arguments.extend_from_slice(rest);
        Ok(Answer::Call {
            function: nth(args, 0),
            arguments,
            then: None,
        })
    }),
    // `(call-then f x k)` answers a call of `f` with `x`, then `k` with its result.
    ("call-then", |g, _, args| {
        let mut arguments = Vec::new();
        arguments.try_reserve(1).map_err(|_| oom(g))?;
        arguments.push(nth(args, 1));
        Ok(Answer::Call {
            function: nth(args, 0),
            arguments,
            then: Some((nth(args, 2), Vec::new())),
        })
    }),
    // `(wait x)` parks the run with the request `x`.
    ("wait", |_, _, args| Ok(Answer::Later(nth(args, 0)))),
    // `(nest)` tries to start a run inside the host call: it must be refused.
    ("nest", |g, s, _| {
        g.evaluate(s, "(add 1 2)").map(Answer::Now)
    }),
    // `(stop-run x)` and `(force-run x)` stop or force the run they are called from, answering `x`.
    ("stop-run", |g, _, args| {
        g.interrupter().stop();
        Ok(Answer::Now(nth(args, 0)))
    }),
    ("force-run", |g, _, args| {
        g.interrupter().force();
        Ok(Answer::Now(nth(args, 0)))
    }),
];

impl Core {
    /// A new interpreter using at most `memory` bytes, or what the process has.
    pub fn new(memory: Option<usize>) -> Result<Core, String> {
        let mut g = Grift::new().map_err(|_| "Grift::new failed".to_string())?;
        if let Some(bytes) = memory {
            g.hook_memory_limit(bytes);
        }
        Ok(Core(g))
    }
    pub fn eval(&mut self, src: &str) -> Result<Value, Error> {
        self.0.evaluate(&mut (), src)
    }
    pub fn call(&mut self, f: &Value, args: &[Value]) -> Result<Value, Error> {
        self.0.call(&mut (), f, args)
    }
    pub fn keep(&mut self, v: Value) -> Result<Kept, Error> {
        self.0.keep(v)
    }
    pub fn kept(&mut self, k: &Kept) -> Result<Value, Error> {
        self.0.kept(k)
    }
    pub fn release(&mut self, k: Kept) -> Result<(), Error> {
        self.0.release(k).map_err(|(e, _)| e)
    }
    /// A stopper for the run in progress, or if none is, the next one to begin.
    pub fn stopper(&self) -> Interrupter {
        self.0.interrupter()
    }
    /// `eval`, in a run that may park.
    pub fn start(&mut self, src: &str) -> Result<Ran, Error> {
        let top = self.0.top_level();
        self.0.start_in(&mut (), &top, "slice", src)
    }
    /// Resume a parked run: its `wait` gives this value, or raises an error of this kind.
    pub fn resume(&mut self, p: Parked, answer: Result<Value, &str>) -> Result<Ran, Error> {
        let answer = answer.map_err(|kind| self.0.error(kind, &[]));
        self.0.resume(&mut (), p, answer).map_err(Error::from)
    }
    /// End a parked run for good, the way grift's hosts do: resume it with a limit error, so
    /// its cleanups run and no handler catches it.
    pub fn abandon(&mut self, p: Parked) -> Result<(), Error> {
        let ending = Error {
            limit: true,
            ..self.0.error("abandoned", &[])
        };
        match self.0.resume(&mut (), p, Err(ending)) {
            Err(e) if e.parked.is_some() => Err(e.error),
            _ => Ok(()),
        }
    }
    /// The printed form of `v` in at most `budget` bytes, and whether it was cut.
    pub fn print(&mut self, v: &Value, budget: usize) -> Result<(String, bool), Error> {
        let mut out = Budget {
            text: TextBuffer::new(),
            left: budget,
            cut: false,
        };
        match self.0.write(v, &mut out) {
            Ok(()) => Ok((out.text.as_str().to_string(), false)),
            Err(_) if out.cut => Ok((out.text.as_str().to_string(), true)),
            Err(e) => Err(e),
        }
    }
    pub fn ending(&self, e: &Error) -> Ending {
        let kind = self.0.symbol_name(&e.kind).unwrap_or("");
        match (e.limit, kind) {
            (_, "out-of-memory") => Ending::OutOfMemory,
            (true, _) => Ending::Interrupted,
            (false, k) => Ending::Error(k.to_string()),
        }
    }
    pub fn trace_len(&self, e: &Error) -> usize {
        e.trace.len()
    }
    /// A report of `e` in at most `budget` bytes, and whether it was cut.
    pub fn describe(&mut self, e: &Error, budget: usize) -> Result<(String, bool), Error> {
        let mut out = TextBuffer::new();
        let status = self.0.describe(e, &mut out, budget)?;
        Ok((out.as_str().to_string(), status == ReportStatus::Truncated))
    }
    /// Refuse the `n`th allocation from now (1 is the next) and every one after; `None` stops.
    pub fn refuse_allocations_from(&mut self, n: Option<usize>) {
        self.0.hook_refuse_from(n);
    }
    /// Allocations asked for so far, in the unit `refuse_allocations_from` counts.
    pub fn allocations(&self) -> usize {
        self.0.hook_allocations()
    }
    pub fn collect_at_every_safe_point(&mut self, on: bool) {
        self.0.hook_stress(on);
    }
}

/// The outcome of `r`.
pub fn outcome(c: &mut Core, r: Result<Value, Error>) -> Outcome {
    match r {
        Ok(v) => match c.print(&v, 1 << 16) {
            Ok((s, _)) => Outcome::Value(s),
            Err(e) => Outcome::Unprintable(c.ending(&e)),
        },
        Err(e) => ending_outcome(c, &e),
    }
}

/// The outcome of a run that may park.
pub fn ran_outcome(c: &mut Core, r: Result<Ran, Error>) -> Outcome {
    match r {
        Ok(Ran::Finished(v)) => outcome(c, Ok(v)),
        Ok(Ran::Parked(p)) => {
            let _ = c.abandon(p);
            Outcome::Parked
        }
        Err(e) => ending_outcome(c, &e),
    }
}

/// The outcome of an error.
pub fn ending_outcome(c: &Core, e: &Error) -> Outcome {
    match c.ending(e) {
        Ending::Error(k) => Outcome::Error(k),
        Ending::Interrupted => Outcome::Interrupted,
        Ending::OutOfMemory => Outcome::OutOfMemory,
    }
}

/// Evaluate `src` and give its outcome.
pub fn run(c: &mut Core, src: &str) -> Outcome {
    let r = c.eval(src);
    outcome(c, r)
}

/// A new interpreter using at most `memory` bytes, with the witnesses' host functions and
/// [`PRELUDE`], or a failed witness.
pub fn fresh(memory: Option<usize>) -> Result<Core, String> {
    let mut c = Core::new(memory)?;
    for &(name, f) in HOSTS {
        let bound = c.0.host_function(name, f).and_then(|v| c.0.define(name, v));
        if let Err(e) = bound {
            return Err(format!("binding {name}: {:?}", c.ending(&e)));
        }
    }
    match run(&mut c, PRELUDE) {
        Outcome::Value(_) => Ok(c),
        o => Err(format!("prelude: {o:?}")),
    }
}

/// Helpers the witnesses' programs use, in the slice language (slice natives and library only).
pub const PRELUDE: &str = r#"
(define! top ((vau! () c c)))
(define! count-list (lambda! (n acc) (if! (same? n 0) acc (count-list (subtract n 1) (pair n acc)))))
(define! nest-list (lambda! (n acc) (if! (same? n 0) acc (nest-list (subtract n 1) (list acc)))))
(define! doubled (lambda! (s k) (if! (same? k 0) s (doubled (join s s) (subtract k 1)))))
(define! spin (lambda! (n) (if! (same? n 0) 0 (spin (subtract n 1)))))
(define! churn (lambda! (n) (if! (same? n 0) 0 (sequence! (list n n n) (churn (subtract n 1))))))
(define! make-error
  (lambda! (kind values)
    ((lambda! (e) (sequence! (bind e (list (quote! kind) (quote! values) (quote! trace)) (list kind values ())) e))
     (context ()))))
#inert
"#;

/// Wall-clock time of `f`.
pub fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let v = f();
    (v, t.elapsed())
}

/// Resident memory of this process in bytes (Linux), 0 elsewhere.
pub fn rss() -> usize {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<usize>().ok())
        .map_or(0, |pages| pages * 4096)
}

/// Runs `f` in a child process (this test binary again, running only the test `test`), so a
/// stack overflow, abort or another test's memory can't touch the rest. In the child, runs `f`.
/// `Err` says how the child failed.
pub fn isolated(
    test: &str,
    case: &str,
    f: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let key = "GRIFT_RULES_CASE";
    match std::env::var(key) {
        // In a child: run only its own case; never spawn again.
        Ok(own) if own == case => return f(),
        Ok(_) => return Ok(()),
        Err(_) => {}
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = std::process::Command::new(exe)
        .args([
            test,
            "--exact",
            "--include-ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env(key, case)
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let tail: Vec<&str> = text.lines().filter(|l| l.starts_with("rules:")).collect();
    if out.status.success() && text.contains("1 passed") {
        if !tail.is_empty() {
            println!("{}", tail.join("\n"));
        }
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err: Vec<&str> = err.lines().rev().take(6).collect();
        Err(format!(
            "case {case}: child ended {} {} {}",
            out.status,
            tail.join("\n"),
            err.join(" | ")
        ))
    }
}

/// Fails with `msg` unless `ok`.
pub fn check(ok: bool, msg: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(msg()) }
}

/// `got` must be `want`.
pub fn expect(what: &str, got: Outcome, want: Outcome) -> Result<(), String> {
    check(got == want, || {
        format!("{what}: got {got:?}, want {want:?}")
    })
}
