//! What takes the machine's and the heap's insides to test: parking on a full heap, under the
//! collector's stress, with the natives off, and generated programs with random points of pause,
//! resume, giving up, stop, force and out-of-memory. The rest is in `tests/parking.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::tests::{kind, show};
use crate::{Answer, Error, Grift, Parked, Ran, Value};

type Log = Vec<String>;

fn note(g: &mut Grift<Log>, log: &mut Log, args: &[Value]) -> Result<Answer, Error> {
    for v in args {
        log.push(show(g, v));
    }
    Ok(Answer::Now(Value::Inert))
}

fn wait(_: &mut Grift<Log>, _: &mut Log, _: &[Value]) -> Result<Answer, Error> {
    Ok(Answer::Later(Value::Inert))
}

/// Uses up every slot the limit leaves, then waits: allocation never collects, so the park finds
/// the heap full.
fn full_wait(g: &mut Grift<Log>, _: &mut Log, _: &[Value]) -> Result<Answer, Error> {
    g.heap.slot_limit = g.heap.len();
    while g.pair(Value::Inert, Value::Inert).is_ok() {}
    Ok(Answer::Later(Value::Inert))
}

fn grift() -> Grift<Log> {
    let mut g: Grift<Log> = Grift::new().unwrap();
    let f = g.host_function("wait", wait).unwrap();
    g.define("wait", f).unwrap();
    let f = g.host_function("full-wait", full_wait).unwrap();
    g.define("full-wait", f).unwrap();
    let f = g.host_function("note", note).unwrap();
    g.define("note", f).unwrap();
    g
}

pub(crate) fn start<T>(g: &mut Grift<T>, state: &mut T, src: &str) -> Result<Ran, Error> {
    let top = g.top_level();
    g.start_in(state, &top, "script.grift", src)
}

/// With no memory left at the drop a run would otherwise be lost: its park made the room.
#[test]
fn dropping_a_handle_asks_for_no_memory() {
    let (mut g, mut log) = (grift(), Log::new());
    let mut handles = Vec::new();
    let src = "(finally! (wait) (note 1))";
    for _ in 0..5 {
        handles.push(parked(start(&mut g, &mut log, src)));
    }
    let room = |g: &Grift<Log>| {
        let queue = g.control.forgotten();
        (queue.capacity(), queue.as_ptr())
    };
    let before = room(&g);
    assert!(before.0 >= 5, "room for {}", before.0);
    drop(handles);
    assert_eq!(room(&g), before);
    while let Some(p) = g.forgotten() {
        give_up(&mut g, &mut log, p);
    }
    assert_eq!(log, ["1"; 5]);
}

/// Gives a parked run up, as a host does: the waiting call answers a limit, and so does each wait
/// of a cleanup on the way out. What the run ended with.
pub(crate) fn give_up<T>(g: &mut Grift<T>, state: &mut T, mut p: Parked) -> Error {
    loop {
        let ending = Error {
            limit: true,
            ..g.error("given-up", &[])
        };
        match g.resume(state, p, Err(ending)) {
            Ok(Ran::Parked(again)) => p = again,
            Ok(Ran::Finished(v)) => panic!("a limit ended with the value {v:?}"),
            Err(e) => {
                assert!(e.parked.is_none(), "{:?}", e.error);
                return e.error;
            }
        }
    }
}

#[track_caller]
pub(crate) fn parked(ran: Result<Ran, Error>) -> Parked {
    match ran {
        Ok(Ran::Parked(p)) => p,
        other => panic!("not parked: {other:?}"),
    }
}

fn resumed(g: &mut Grift<Log>, log: &mut Log, p: Parked, v: Value) -> Result<Ran, Error> {
    g.resume(log, p, Ok(v)).map_err(|e| {
        assert!(e.parked.is_none());
        e.error
    })
}

#[test]
fn parking_inside_a_native_twin_cannot_be_told_from_its_definition() {
    for src in [
        "(if! (wait) (note 1) (note 2))",
        "(if! #true (wait) 0)",
        "(if! #false 0 (wait))",
        "(if! 1 (wait) 0)",
        "(sequence! (wait) (wait) (first (wait)))",
        "(sequence! (note 1) (wait))",
        // `first` by name: by value it would print, and a native prints unlike its definition.
        "(evaluate (list (quote! if!) (list wait) (list (quote! first) 1) 0) (current-context))",
        "(search (wait) \"a\" 0)",
        "(map (lambda! (x) (if! (wait) x (first x))) (list 1 2))",
        "(finally! (sequence! (wait) (first 1)) (if! (wait) (note 1) (note 2)))",
    ] {
        let mut seen = Vec::new();
        for natives in [true, false] {
            let mut g = grift();
            if !natives {
                g.run_definitions();
            }
            let mut log = Vec::new();
            let mut ran = start(&mut g, &mut log, src);
            let mut waits = 0;
            while let Ok(Ran::Parked(p)) = ran {
                waits += 1;
                ran = resumed(&mut g, &mut log, p, Value::Boolean(true));
            }
            let mut text = String::new();
            match ran {
                Ok(Ran::Finished(v)) => text = show(&g, &v),
                Err(e) => g.describe(&e, &mut text, 4096).map(drop).unwrap(),
                Ok(Ran::Parked(_)) => panic!("the loop took these"),
            }
            seen.push((waits, text, log));
        }
        assert_eq!(seen[0], seen[1], "{src}");
    }
}

#[test]
fn a_run_with_no_memory_to_park_fails_there_and_its_cleanups_run() {
    let (mut g, mut log) = (grift(), Vec::new());
    g.heap.kept_limit = 0;
    let e = start(&mut g, &mut log, "(finally! (full-wait) (note 1))").unwrap_err();
    assert_eq!(kind(&g, &e), "out-of-memory");
    assert_eq!(log, ["1"]);
    assert_eq!(g.parked(), 0);
    assert_eq!(g.evaluate(&mut log, "(add 1 2)"), Ok(Value::Integer(3)));
}

/// A park goes into the kept table, so a full heap doesn't stop it, and a resume out of it.
#[test]
fn parking_and_resuming_need_no_heap_slot() {
    let (mut g, mut log) = (grift(), Vec::new());
    let p = parked(start(&mut g, &mut log, "(add 1 (full-wait))"));
    g.heap.refuse_in = Some(0);
    let Ok(Ran::Finished(v)) = resumed(&mut g, &mut log, p, Value::Integer(1)) else {
        panic!("a resume needs no slot");
    };
    assert_eq!((v, g.heap.refused), (Value::Integer(2), 0));
}

/// Out of memory, with only garbage in the heap: the unwinding collects first, so each cleanup has
/// the memory to run.
#[test]
fn out_of_memory_collects_before_its_cleanups_run() {
    let (mut g, mut log) = (grift(), Vec::new());
    let p = parked(start(
        &mut g,
        &mut log,
        "(finally! (finally! (sequence! (wait) (pair 1 2) (note 0)) (note 1)) (note 2))",
    ));
    g.heap.slot_limit = g.heap.len();
    while g.pair(Value::Inert, Value::Inert).is_ok() {}
    g.heap.not_due();
    let e = g.resume(&mut log, p, Ok(Value::Inert)).unwrap_err();
    assert_eq!(kind(&g, &e.error), "out-of-memory");
    assert_eq!(log, ["1", "2"]);
}

/// A cleanup whose first step has no memory: the error goes on to the next, as from any step of
/// that cleanup, and doesn't end the run's unwinding there.
#[test]
fn a_cleanup_whose_first_step_is_refused_is_passed_over_for_the_next() {
    // The run errs on, or is given up: it is the same cleanups either way.
    for given_up in [false, true] {
        let mut passed_over = false;
        for refuse_in in 0..16 {
            let (mut g, mut log) = (grift(), Vec::new());
            let p = parked(start(
                &mut g,
                &mut log,
                "(finally! (finally! (sequence! (wait) (first 0)) (note 1)) (note 2))",
            ));
            let answer = match given_up {
                true => Err(Error {
                    limit: true,
                    ..g.error("given-up", &[])
                }),
                false => Ok(Value::Inert),
            };
            g.heap.refuse_in = Some(refuse_in);
            g.heap.refuse_once = true;
            let e = g.resume(&mut log, p, answer).unwrap_err().error;
            passed_over |= kind(&g, &e) == "out-of-memory" && log == ["2"];
            assert_eq!(g.parked(), 0);
        }
        assert!(passed_over, "given up: {given_up}");
    }
}

/// A resumed run has let go of what it asked for: a big request isn't held for the rest of the run.
#[test]
fn a_resumed_run_holds_its_request_no_longer() {
    let (mut g, mut log) = (grift(), Vec::new());
    let ask = g
        .host_function("ask", |_, _, args| Ok(Answer::Later(args[0].clone())))
        .unwrap();
    g.define("ask", ask).unwrap();
    let held = g
        .host_function("held", |g, log: &mut Log, _| {
            log.push(format!("{:?}", g.machine.request));
            Ok(Answer::Now(Value::Inert))
        })
        .unwrap();
    g.define("held", held).unwrap();
    let p = parked(start(&mut g, &mut log, "(ask (list 1)) (held)"));
    assert!(matches!(
        g.resume(&mut log, p, Ok(Value::Inert)),
        Ok(Ran::Finished(_))
    ));
    assert_eq!(log, ["Inert"]);
}

/// Every collection walks a parked run, so it brings the next one no nearer than its size says.
#[test]
fn a_parked_run_counts_for_its_size_when_the_collector_decides() {
    let (mut g, mut log) = (grift(), Vec::new());
    let live = |g: &mut Grift<Log>, log: &mut Log| {
        g.heap.request_collection();
        g.evaluate(log, "#inert").unwrap();
        g.heap.live()
    };
    let p = parked(start(
        &mut g,
        &mut log,
        "(define! down (lambda! (n) (if! (same? n 0) (wait) (add 1 (down (subtract n 1))))))
         (down 50000)",
    ));
    let waiting = live(&mut g, &mut log);
    let ended = give_up(&mut g, &mut log, p);
    assert_eq!(kind(&g, &ended), "given-up");
    let gone = live(&mut g, &mut log);
    // A context a level, and at least two frames.
    assert!(waiting - gone >= 3 * 50_000, "{waiting} then {gone}");
}

/// Parked runs in every state a machine has when it parks, each holding data only it reaches,
/// taken up in shuffled order with collections between: all come back whole, and round after
/// round the heap stays the size it was.
#[test]
fn collection_loses_nothing_a_parked_run_holds_and_keeps_nothing_after() {
    let (mut g, mut log) = (grift(), Vec::new());
    g.evaluate(
        &mut log,
        "(define! churn (lambda! () (length (map list (range 0 64)))))
         (define! id (lambda! (n) (list n (join \"run\" (integer-to-string n)))))",
    )
    .unwrap();
    let shapes: [fn(usize) -> (String, String); 5] = [
        |n| {
            let src = format!("(let! ((held (id {n}))) (wait) held)");
            (src, format!("({n} \"run{n}\")"))
        },
        // Arguments half evaluated, on the argument stack alone.
        |n| {
            let src = format!("(list (id {n}) (wait) (id {n}))");
            (src, format!("(({n} \"run{n}\") 0 ({n} \"run{n}\"))"))
        },
        // A map's list half built.
        |n| {
            let src = format!("(map (lambda! (x) (if! (same? x 1) (wait) (id x))) (list {n} 1))");
            (src, format!("(({n} \"run{n}\") 0)"))
        },
        // An error waiting for a cleanup: its values and trace are in `waiting` alone.
        |n| {
            let src = format!(
                "(catch! (finally! (raise (error (quote! mine) (id {n}))) (wait))
                         e (evaluate-in! values e))"
            );
            (src, format!("(({n} \"run{n}\"))"))
        },
        // A cleanup's frame, its value in `waiting`.
        |n| {
            let src = format!("(finally! (id {n}) (wait))");
            (src, format!("({n} \"run{n}\")"))
        },
    ];
    let mut seed = 12345u64;
    let mut slots = Vec::new();
    for _ in 0..12 {
        let mut waiting = Vec::new();
        for n in 2..42 {
            let (src, expected) = shapes[n % shapes.len()](n);
            waiting.push((parked(start(&mut g, &mut log, &src)), expected));
            if n % 8 == 0 {
                g.heap.request_collection();
                g.evaluate(&mut log, "(churn)").unwrap();
            }
        }
        assert_eq!(g.parked(), 40);
        while !waiting.is_empty() {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let i = (seed >> 33) as usize % waiting.len();
            let (p, expected) = waiting.swap_remove(i);
            g.heap.request_collection();
            g.evaluate(&mut log, "(churn)").unwrap();
            match resumed(&mut g, &mut log, p, Value::Integer(0)) {
                Ok(Ran::Finished(v)) => assert_eq!(show(&g, &v), expected),
                other => panic!("{expected}: {other:?}"),
            }
        }
        assert_eq!(g.parked(), 0);
        slots.push(g.heap.len());
    }
    // Early rounds settle as collections fall at other moments; then it never moves.
    assert_eq!(slots[6], slots[11], "{slots:?}");
}

/// A real out-of-memory in a resumed run passes its handler, which then would have had room.
#[test]
fn real_out_of_memory_passes_handlers() {
    let (mut g, mut log) = (grift(), Vec::new());
    let p = parked(start(
        &mut g,
        &mut log,
        "(catch! (sequence! (wait) (pair 1 2)) #ignore (note 0))",
    ));
    g.heap.refuse_in = Some(0);
    g.heap.refuse_once = true;
    let e = g.resume(&mut log, p, Ok(Value::Inert)).unwrap_err().error;
    assert_eq!(kind(&g, &e), "out-of-memory");
    assert!(e.limit);
    assert!(log.is_empty(), "{log:?}");
}

/// Generated programs, run two ways: each alone with its waits answered at once, which is what
/// it means; and together in one interpreter, parked and taken up in random order, with the
/// collector and the natives set at random, and with random stops, forces, runs given up and allocation
/// failures, and programs forging the names of limits. Whatever happens to one run, no other notices. The first way decides what a run
/// that nothing touched must do, down to its trace, and how far one that something touched
/// must get; the cleanups it entered say, by themselves, whether they all ran.
mod generated {
    use super::*;
    use crate::Interrupter;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 16
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        fn one_in(&mut self, n: u64) -> bool {
            self.below(n) == 0
        }
    }

    /// Writes one expression. Every number a program logs or waits on starts with its run, so
    /// a wait says whose it is, and a cleanup's number is its own.
    struct Program<'a> {
        r: &'a mut Rng,
        run: u64,
        next: u64,
        /// The most a list it makes may hold.
        churn: u64,
    }

    impl Program<'_> {
        fn id(&mut self) -> u64 {
            self.next += 1;
            self.run * 100_000 + self.next
        }

        fn expression(&mut self, depth: u32) -> String {
            let choice = self.r.below(if depth == 0 { 4 } else { 22 });
            let id = self.id();
            let sub = |p: &mut Self| p.expression(depth.saturating_sub(1));
            match choice {
                0 | 1 => format!("(wait (list {id} \"asked\"))"),
                2 => format!("(note {id})"),
                3 => format!("{id}"),
                4 => format!("(add {} {})", sub(self), sub(self)),
                5 => format!("(sequence! {} {})", sub(self), sub(self)),
                6 => format!(
                    "(if! (same? {} {id}) {} {})",
                    sub(self),
                    sub(self),
                    sub(self)
                ),
                7 => format!("(let! ((x {})) (note x) x)", sub(self)),
                8..=10 => {
                    let cleanup = match self.r.below(3) {
                        0 => format!("(note -{id})"),
                        1 => format!("(sequence! (note -{id}) (wait (list {id} \"cleanup\")))"),
                        _ => format!("(sequence! (note -{id}) (first 0))"),
                    };
                    format!(
                        "(finally! (sequence! (enter {id}) {}) {cleanup})",
                        sub(self)
                    )
                }
                11 => format!("(catch! {} e {})", sub(self), handler(id)),
                12 => format!("(map (lambda! (x) {}) (list 1 2))", sub(self)),
                13 => format!("(for-each (lambda! (x) {}) (list 1 2))", sub(self)),
                14 => match self.r.below(2) {
                    0 => "(first 0)".to_string(),
                    _ => format!("(raise (error (quote! mine) {id}))"),
                },
                15 => format!("(length (map list (range 0 {})))", self.r.below(self.churn)),
                16 => format!("(nest (quote! {}))", sub(self)),
                // A forgery of a limit's name: caught if something can catch it, only data if not.
                17 => format!("(raise (error (quote! {}) (quote! forged)))", forged(id)),
                18 => format!(
                    "(catch! (raise (error (quote! {}) (quote! forged))) e {})",
                    forged(id),
                    handler(id)
                ),
                // A limit of the host's own: nothing catches it, in a run nothing else touched too.
                19 => format!("(catch! (host-limit {id}) e {})", handler(id)),
                _ => format!("(list (join \"held\" \"{id}\") {} (list {id}))", sub(self)),
            }
        }
    }

    fn forged(id: u64) -> &'static str {
        match id % 3 {
            0 => "interrupted",
            1 => "out-of-memory",
            _ => "host-limit",
        }
    }

    /// What a `catch!` runs. A limit has no business here, whatever it is called.
    fn handler(id: u64) -> String {
        format!("(seen {id} (evaluate-in! kind e) (evaluate-in! values e))")
    }

    /// The answer to a wait: it depends only on what was asked.
    fn answer(g: &mut Grift<World>, id: i64) -> Result<Value, Error> {
        match id % 5 {
            0 => Err(g.error("timed-out", &[Value::Integer(id)])),
            1 => g.string(&format!("answer{id}")),
            _ => Ok(Value::Integer(id * 2)),
        }
    }

    #[derive(Default)]
    struct World {
        /// What each run logged, and the run in progress.
        logs: Vec<Log>,
        current: usize,
        /// Answer at once instead of parking, where parking would be allowed.
        at_once: bool,
        /// Host calls so far, and at which to stop.
        calls: u64,
        stop_at: Option<(u64, bool)>,
        /// A handle taken for the first run, long over: it reaches none of these.
        old: Option<Interrupter>,
        /// How many stops reached each run, and how long its log was at the first.
        stops: Vec<u64>,
        stopped_at: Vec<Option<usize>>,
        /// How long each run's log was at its force.
        forced_at: Vec<Option<usize>>,
    }

    impl World {
        /// A host call, after it has logged: what is armed lands as it returns.
        fn call(&mut self, g: &Grift<World>) {
            self.calls += 1;
            let Some((at, force)) = self.stop_at else {
                return;
            };
            let Some(run) = self.logs.get(self.current) else {
                return;
            };
            if self.calls < at {
                return;
            }
            self.stop_at = None;
            let len = run.len();
            let current = self.current;
            // Each with a handle taken for the run in progress.
            if force {
                assert!(g.interrupter().force(), "a run is in progress");
                self.forced_at[current].get_or_insert(len);
            } else if g.interrupter().stop() {
                self.stops[current] += 1;
                self.stopped_at[current].get_or_insert(len);
            }
        }

        fn log(&mut self, line: String) {
            if let Some(log) = self.logs.get_mut(self.current) {
                log.push(line);
            }
        }
    }

    fn g_note(g: &mut Grift<World>, w: &mut World, args: &[Value]) -> Result<Answer, Error> {
        for v in args {
            w.log(show(g, v));
        }
        w.call(g);
        Ok(Answer::Now(Value::Inert))
    }

    fn g_enter(g: &mut Grift<World>, w: &mut World, args: &[Value]) -> Result<Answer, Error> {
        if let [Value::Integer(id)] = args {
            w.log(format!("+{id}"));
        }
        w.call(g);
        Ok(Answer::Now(Value::Inert))
    }

    /// `(seen id kind values)`: a handler ran. Only what carries the forger's mark may have a
    /// limit's name; the real thing would have passed.
    fn g_seen(g: &mut Grift<World>, w: &mut World, args: &[Value]) -> Result<Answer, Error> {
        let [Value::Integer(id), Value::Symbol(kind), values] = args else {
            panic!("{args:?}");
        };
        let name = g.symbol_name(kind)?.to_string();
        let forged = g.list_items(values)?.iter().any(
            |v| matches!(v, Value::Symbol(s) if g.symbol_name(s).is_ok_and(|n| n == "forged")),
        );
        let limit = ["interrupted", "out-of-memory", "given-up", "host-limit"];
        assert!(
            forged || !limit.contains(&name.as_str()),
            "a handler saw {name}"
        );
        w.log(id.to_string());
        w.call(g);
        Ok(Answer::Now(Value::Inert))
    }

    fn g_host_limit(g: &mut Grift<World>, w: &mut World, args: &[Value]) -> Result<Answer, Error> {
        if let [Value::Integer(id)] = args {
            w.log(format!("${id}"));
        }
        w.call(g);
        Err(Error {
            limit: true,
            ..g.error("host-limit", args)
        })
    }

    fn g_wait(g: &mut Grift<World>, w: &mut World, args: &[Value]) -> Result<Answer, Error> {
        w.call(g);
        if w.at_once && g.can_park() {
            let id = request_id(g, &args[0]);
            return answer(g, id).map(Answer::Now);
        }
        Ok(Answer::Later(args[0].clone()))
    }

    /// `(nest form)`: the machine evaluates `form` at top level for this host function.
    fn g_nest(g: &mut Grift<World>, w: &mut World, args: &[Value]) -> Result<Answer, Error> {
        w.call(g);
        let (top, evaluate) = (g.top_level(), g.symbol("evaluate")?);
        Ok(Answer::Call {
            function: g.lookup(&top, &evaluate)?.unwrap(),
            arguments: vec![args[0].clone(), top],
            then: None,
        })
    }

    fn request_id<T>(g: &Grift<T>, request: &Value) -> i64 {
        let items = g
            .list_items(request)
            .expect("a waiting run holds its request");
        let Value::Integer(id) = items[0] else {
            panic!("{items:?}");
        };
        assert!(
            matches!(g.as_str(&items[1]), Ok(Some("asked" | "cleanup"))),
            "{items:?}"
        );
        id
    }

    fn interpreter() -> Grift<World> {
        let mut g: Grift<World> = Grift::new().unwrap();
        let f = g.host_function("wait", g_wait).unwrap();
        g.define("wait", f).unwrap();
        let f = g.host_function("note", g_note).unwrap();
        g.define("note", f).unwrap();
        let f = g.host_function("enter", g_enter).unwrap();
        g.define("enter", f).unwrap();
        let f = g.host_function("nest", g_nest).unwrap();
        g.define("nest", f).unwrap();
        let f = g.host_function("seen", g_seen).unwrap();
        g.define("seen", f).unwrap();
        let f = g.host_function("host-limit", g_host_limit).unwrap();
        g.define("host-limit", f).unwrap();
        g
    }

    /// A finished run's result, as text; an error with its kind.
    #[derive(Clone, Debug, PartialEq)]
    struct Outcome {
        kind: Option<String>,
        text: String,
        limit: bool,
    }

    fn outcome<T>(g: &Grift<T>, done: Result<Value, &Error>) -> Outcome {
        match done {
            Ok(v) => Outcome {
                kind: None,
                text: show(g, &v),
                limit: false,
            },
            Err(e) => {
                let mut text = String::new();
                g.describe(e, &mut text, 4096).unwrap();
                Outcome {
                    kind: Some(kind(g, e)),
                    text,
                    limit: e.limit,
                }
            }
        }
    }

    /// What a program means: alone, its waits answered at once.
    fn meaning(run: usize, src: &str) -> (Outcome, Log) {
        let mut g = interpreter();
        let mut w = World {
            logs: vec![Vec::new(); run + 1],
            current: run,
            at_once: true,
            stops: vec![0; run + 1],
            stopped_at: vec![None; run + 1],
            forced_at: vec![None; run + 1],
            ..World::default()
        };
        let top = g.top_level();
        let name = format!("run{run}.grift");
        let done = g.start_in(&mut w, &top, &name, src);
        let out = match done {
            Ok(Ran::Finished(v)) => outcome(&g, Ok(v)),
            Ok(Ran::Parked(_)) => panic!("{src}: answered at once, and parked"),
            Err(e) => outcome(&g, Err(&e)),
        };
        // Nothing else touched it: a name makes nothing real, and only the host's own limit is one.
        let log = std::mem::take(&mut w.logs[run]);
        let by_host = log.iter().any(|l| l.starts_with('$'));
        assert_eq!(out.limit, by_host, "{out:?}\n{src}");
        (out, log)
    }

    /// The most cleanups entered and never run, from the log alone, and that those that ran ran in
    /// order. `+n` enters a cleanup and `-n` is its first act. With `free`, a cleanup may have no
    /// `+n`: what cut the run came between its install and its body's first step.
    fn cleanups_missed(log: &[String], free: bool, context: &str) -> usize {
        let mut stack = Vec::new();
        let mut missed = 0;
        for line in log {
            if let Some(id) = line.strip_prefix('-') {
                if !stack.contains(&id) {
                    assert!(free, "{id} cleaned up, never entered: {log:?}\n{context}");
                    continue;
                }
                while let Some(top) = stack.pop() {
                    if top == id {
                        break;
                    }
                    missed += 1;
                }
            } else if let Some(id) = line.strip_prefix('+') {
                stack.push(id);
            }
        }
        missed + stack.len()
    }

    struct Waiting {
        run: usize,
        parked: Parked,
    }

    /// How far something touched a run, for the checks at its end.
    #[derive(Clone, Default)]
    struct Touched {
        given_up: bool,
        /// Allocations refused while it was in progress, and whether they went on being.
        refused: usize,
        refused_for_good: bool,
        /// How long its log was before any of this. A log is the run's meaning up to there.
        clean_until: Option<usize>,
    }

    fn check_end(
        run: usize,
        got: &Outcome,
        world: &World,
        meaning: &(Outcome, Log),
        touched: &Touched,
        context: &str,
    ) {
        let (log, stops, stopped_at, forced_at) = (
            world.logs[run].as_slice(),
            world.stops[run],
            world.stopped_at[run],
            world.forced_at[run],
        );
        let context = format!("run {run}: {got:?}\nlog {log:?}\nmeaning {meaning:?}\n{context}");
        if let Some(kind) = &got.kind {
            assert_ne!(kind, "stale-value", "{context}");
            assert_ne!(kind, "foreign-value", "{context}");
        }
        let cut = stops > 0 || touched.refused > 0 || forced_at.is_some();
        let clean_until = [touched.clean_until, stopped_at, forced_at]
            .into_iter()
            .flatten()
            .min();
        if !touched.given_up && !cut {
            assert_eq!(got, &meaning.0, "{context}");
            assert_eq!(log, meaning.1.as_slice(), "{context}");
        } else {
            let n = clean_until.unwrap_or(log.len()).min(meaning.1.len());
            assert_eq!(log.get(..n), meaning.1.get(..n), "{context}");
        }
        // A force is the end, whatever else happens: nothing more runs, and it names the ending.
        if let Some(at) = forced_at {
            assert_eq!(got.kind.as_deref(), Some("interrupted"), "{context}");
            assert!(got.limit, "{context}");
            assert_eq!(log.len(), at, "ran after a force\n{context}");
            return;
        }
        if (touched.given_up || cut) && got != &meaning.0 {
            let mut allowed = Vec::new();
            if touched.given_up {
                allowed.push("given-up");
            }
            if stops > 0 {
                allowed.push("interrupted");
            }
            if touched.refused > 0 {
                allowed.push("out-of-memory");
            }
            if log.iter().any(|l| l.starts_with('$')) {
                allowed.push("host-limit");
            }
            let kind = got.kind.as_deref().unwrap_or("a value");
            assert!(
                allowed.contains(&kind),
                "{kind} not in {allowed:?}\n{context}"
            );
            assert!(
                got.limit,
                "only a limit can replace what a run meant\n{context}"
            );
        }
        let free = cut;
        let missed = cleanups_missed(log, free, &context);
        if !touched.refused_for_good {
            let budget = stops as usize + touched.refused;
            assert!(missed <= budget, "missed {missed} of {budget}\n{context}");
        }
    }

    /// A stop or a refusal can only end a run with what it is, or leave its meaning alone.
    #[test]
    #[should_panic(expected = "a value not in")]
    fn a_stopped_run_meant_to_err_may_not_end_with_a_value() {
        let world = World {
            logs: vec![Vec::new()],
            stops: vec![1],
            stopped_at: vec![None],
            forced_at: vec![None],
            ..World::default()
        };
        let meant = Outcome {
            kind: Some("mine".to_string()),
            text: "mine".to_string(),
            limit: false,
        };
        let got = Outcome {
            kind: None,
            text: "1".to_string(),
            limit: false,
        };
        check_end(
            0,
            &got,
            &world,
            &(meant, Vec::new()),
            &Touched::default(),
            "",
        );
    }

    /// What the cases reached, so that a run can say it tested something.
    #[derive(Debug, Default)]
    struct Reached {
        runs: usize,
        parks: usize,
        given_up: usize,
        stopped: usize,
        forced: usize,
        refused: usize,
        /// Programs that raise a limit's name themselves, and that call the host's own limit.
        forgeries: usize,
        host_limits: usize,
    }

    fn one_case(seed: u64, stress: bool, reached: &mut Reached) {
        let mut r = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let runs = 1 + r.below(5) as usize;
        let programs: Vec<String> = (0..runs)
            .map(|run| {
                let mut p = Program {
                    r: &mut r,
                    run: run as u64 + 1,
                    next: 0,
                    churn: if stress { 6 } else { 200 },
                };
                let depth = if stress { 3 } else { 5 };
                p.expression(depth)
            })
            .collect();
        let with = |text: &str| programs.iter().filter(|p| p.contains(text)).count();
        reached.forgeries += with("(quote! forged)");
        reached.host_limits += with("(host-limit ");
        let context = format!("seed {seed} stress {stress}\n{programs:#?}");
        let meanings: Vec<(Outcome, Log)> = programs
            .iter()
            .enumerate()
            .map(|(run, src)| meaning(run, src))
            .collect();

        let mut g = interpreter();
        if r.one_in(4) {
            g.run_definitions();
        }
        let mut w = World {
            logs: vec![Vec::new(); runs],
            current: usize::MAX,
            stops: vec![0; runs],
            stopped_at: vec![None; runs],
            forced_at: vec![None; runs],
            ..World::default()
        };
        w.old = Some(g.interrupter());
        g.evaluate(
            &mut w,
            "(define! churn (lambda! (n) (length (map list (range 0 n)))))",
        )
        .unwrap();
        g.heap.request_collection();
        g.evaluate(&mut w, "#inert").unwrap();
        let baseline = g.heap.live();
        g.heap.stress = stress;

        let may_stop = r.one_in(2);
        let may_force = r.one_in(3);
        let may_refuse = r.one_in(2);
        let may_give_up = r.one_in(2);
        let mut touched = vec![Touched::default(); runs];
        let mut waiting: Vec<Waiting> = Vec::new();
        let mut next_run = 0;
        let mut refusal_ends_in = 0u64;
        let mut ended = 0;

        // What a run came to: parked again, or at its end, and checked.
        macro_rules! came_to {
            ($run:expr, $done:expr) => {{
                let run = $run;
                let done: Result<Ran, Error> = $done;
                let got = match done {
                    Ok(Ran::Parked(parked)) => {
                        reached.parks += 1;
                        assert_eq!(
                            request_id(&g, parked.request()) as u64 / 100_000,
                            run as u64 + 1,
                            "{context}"
                        );
                        waiting.push(Waiting { run, parked });
                        None
                    }
                    Ok(Ran::Finished(v)) => Some(outcome(&g, Ok(v))),
                    Err(e) => Some(outcome(&g, Err(&e))),
                };
                if let Some(got) = got {
                    ended += 1;
                    reached.runs += 1;
                    reached.given_up += usize::from(touched[run].given_up);
                    reached.stopped += usize::from(w.stops[run] > 0);
                    reached.forced += usize::from(w.forced_at[run].is_some());
                    reached.refused += usize::from(touched[run].refused > 0);
                    check_end(run, &got, &w, &meanings[run], &touched[run], &context);
                }
            }};
        }

        let mut ticks = 0u64;
        while ended < runs {
            ticks += 1;
            assert!(ticks < 100_000, "the runs never end\n{context}");
            if refusal_ends_in > 0 {
                refusal_ends_in -= 1;
                if refusal_ends_in == 0 {
                    g.heap.refuse_in = None;
                }
            }
            let refused_before = g.heap.refused;
            let for_good = g.heap.refuse_in.is_some() && !g.heap.refuse_once;
            // Refusals that came while `run` was in progress are its.
            macro_rules! charge {
                ($run:expr, $log_before:expr) => {
                    if g.heap.refused != refused_before {
                        let t: &mut Touched = &mut touched[$run];
                        t.refused += g.heap.refused - refused_before;
                        t.refused_for_good |= for_good;
                        t.clean_until =
                            Some(t.clean_until.map_or($log_before, |c| c.min($log_before)));
                    }
                };
            }
            match r.below(11) {
                // Start the next.
                0 | 1 if next_run < runs => {
                    let run = next_run;
                    next_run += 1;
                    w.current = run;
                    let top = g.top_level();
                    let name = format!("run{run}.grift");
                    let done = g.start_in(&mut w, &top, &name, &programs[run]);
                    charge!(run, 0);
                    came_to!(run, done);
                }
                // Give one up.
                2 if may_give_up && !waiting.is_empty() => {
                    let i = r.below(waiting.len() as u64) as usize;
                    let Waiting { run, parked } = waiting.swap_remove(i);
                    w.current = run;
                    touched[run].given_up = true;
                    let at = w.logs[run].len();
                    touched[run].clean_until.get_or_insert(at);
                    let ending = Error {
                        limit: true,
                        ..g.error("given-up", &[])
                    };
                    let done = g.resume(&mut w, parked, Err(ending)).map_err(|e| {
                        assert!(e.parked.is_none(), "{:?}\n{context}", e.error);
                        e.error
                    });
                    charge!(run, at);
                    came_to!(run, done);
                }
                // Lose a handle and find it again.
                3 if !waiting.is_empty() => {
                    let i = r.below(waiting.len() as u64) as usize;
                    let Waiting { run, parked } = waiting.swap_remove(i);
                    let queue = g.control.forgotten();
                    assert!(queue.len() < queue.capacity(), "{context}");
                    drop(queue);
                    drop(parked);
                    let parked = g.forgotten().expect("the handle dropped");
                    let asked = request_id(&g, parked.request()) as u64 / 100_000;
                    assert_eq!(asked, run as u64 + 1, "{context}");
                    waiting.push(Waiting { run, parked });
                }
                // Other work in the heap.
                4 => {
                    w.current = usize::MAX;
                    g.heap.stress = false;
                    let _ = g.evaluate(&mut w, "(churn 300)");
                    g.heap.stress = stress;
                }
                5 => {
                    w.current = usize::MAX;
                    g.heap.request_collection();
                    let _ = g.evaluate(&mut w, "#inert");
                    // No run is in progress, and a handle for another finds nothing.
                    assert!(!g.interrupter().stop(), "{context}");
                    assert!(!w.old.as_mut().unwrap().stop(), "{context}");
                    assert!(!w.old.as_mut().unwrap().force(), "{context}");
                }
                // Arm a stop, a force or a refusal, for what comes next.
                6 if may_stop => w.stop_at = Some((w.calls + r.below(6), false)),
                8 if may_force => w.stop_at = Some((w.calls + r.below(8), true)),
                7 if may_refuse && g.heap.refuse_in.is_none() => {
                    g.heap.refuse_in = Some(r.below(48) as usize);
                    g.heap.refuse_once = r.one_in(2);
                    if !g.heap.refuse_once {
                        refusal_ends_in = 1 + r.below(3);
                    }
                }
                // Take one up.
                _ if !waiting.is_empty() => {
                    let i = r.below(waiting.len() as u64) as usize;
                    let Waiting { run, parked } = waiting.swap_remove(i);
                    w.current = run;
                    let id = request_id(&g, parked.request());
                    let at = w.logs[run].len();
                    let answered = answer(&mut g, id);
                    let done = g.resume(&mut w, parked, answered).map_err(|e| {
                        assert!(e.parked.is_none(), "{:?}\n{context}", e.error);
                        e.error
                    });
                    charge!(run, at);
                    came_to!(run, done);
                }
                _ => {}
            }
        }

        g.heap.refuse_in = None;
        g.heap.stress = false;
        assert_eq!(g.parked(), 0, "{context}");
        assert!(g.forgotten().is_none(), "{context}");
        w.current = usize::MAX;
        w.stop_at = None;
        g.heap.request_collection();
        assert_eq!(
            g.evaluate(&mut w, "(add 1 2)"),
            Ok(Value::Integer(3)),
            "{context}"
        );
        g.heap.request_collection();
        g.evaluate(&mut w, "#inert").unwrap();
        assert!(
            g.heap.live() <= baseline + 64,
            "kept {} over {baseline}\n{context}",
            g.heap.live()
        );
    }

    /// Fixed seeds and few cases here; `GRIFT_PARK_SEED` and `GRIFT_PARK_CASES` run more.
    #[test]
    fn whatever_happens_to_one_run_the_rest_mean_what_they_mean() {
        let number = |name: &str, default: u64| {
            std::env::var(name)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default)
        };
        let first = number("GRIFT_PARK_SEED", 1);
        let cases = number("GRIFT_PARK_CASES", 60);
        let mut reached = Reached::default();
        for seed in first..first + cases {
            // One case in eight collects at every step: slow, and it reaches what a parked run holds.
            one_case(seed, seed % 8 == 0, &mut reached);
        }
        // `--nocapture` shows this. A fixed seed, so these can be required: each kind of thing
        // happened to some run, whatever the program generator comes to do.
        println!("{cases} cases from seed {first}: {reached:?}");
        if cases >= 60 {
            assert!(reached.parks > cases as usize, "{reached:?}");
            assert!(
                [
                    reached.given_up,
                    reached.stopped,
                    reached.forced,
                    reached.refused,
                    reached.forgeries,
                    reached.host_limits
                ]
                .iter()
                .all(|&n| n > 5),
                "{reached:?}"
            );
        }
    }
}
