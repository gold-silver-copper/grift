//! The memory limit a host sets, through the public API alone: what it refuses, what it leaves
//! alone and what it counts. A scenario each, because the limit and the count are the host's
//! calls: no program can set or read them, so no generated program or native's twin can say this.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "a test may abort when memory is refused"
)]
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use grift::{Answer, Grift, Kind, Value};

const MIB: usize = 1 << 20;

/// A program over the limit ends with the limit `out-of-memory`, whichever storage it fills, with
/// no more held than the limit; and the interpreter then runs the next program, every time. No
/// allocator refuses anything here: the limit is far below what the system would give.
#[test]
fn a_program_over_the_limit_is_refused_and_the_next_one_runs() {
    let small = "(list (add 1 2) (join \"a\" \"b\") (length (range 0 100)))";
    let runaways = [
        // Pairs: the heap's chunks.
        "(define! hog (lambda! (xs n) (hog (pair n xs) (add n 1)))) (hog () 0)",
        // A string that doubles, and a bytes value.
        "(define! hog (lambda! (s) (hog (join s s)))) (hog \"0123456789abcdef\")",
        "(define! hog (lambda! (b) (hog (join b b)))) (hog #bytes(1 2 3 4 5 6 7 8))",
        // Names, and the bindings of one context.
        "(define! c (context ()))
         (define! hog (lambda! (i) (bind c (string-to-symbol (integer-to-string i)) i) (hog (add i 1))))
         (hog 0)",
        // Calls that never return: the machine's stacks, with each call's context.
        "(define! hog (lambda! (n) (add 1 (hog n)))) (hog 0)",
    ];
    for program in runaways {
        let mut g: Grift = Grift::new().unwrap();
        g.evaluate(&mut (), small).unwrap();
        let limit = g.memory_held() + 8 * MIB;
        g.set_memory_limit(Some(limit));
        for _ in 0..3 {
            let e = g.evaluate(&mut (), program).unwrap_err();
            assert_eq!(e.grift_kind(), Some(Kind::OutOfMemory), "{program}");
            assert!(e.limit && g.memory_held() <= limit, "{program}");
            let ran = (0..20).filter(|_| g.evaluate(&mut (), small).is_ok());
            assert_eq!(ran.count(), 20, "after {program}");
        }
    }
}

/// A program that fits the limit is refused nothing, on every run: what a run leaves is garbage,
/// collected before the limit refuses anything for it. `range` is 30,000 calls deep, so its
/// frames and contexts are most of what it needs; the strings double to a megabyte, each garbage
/// as soon as the next is made.
#[test]
fn a_program_under_the_limit_runs_as_with_none() {
    let small = "(length (list (add 1 2) (join \"a\" \"b\") (length (range 0 100))))";
    let strings = "(define! m (lambda! (s n) (if! (less-than? n 20) (m (join s s) (add n 1)) n)))
        (define! loop (lambda! (i) (if! (less-than? i 40) (sequence! (m \"x\" 0) (loop (add i 1))) i)))
        (loop 0)";
    for (program, room) in [
        (small, 200_000),
        ("(length (range 0 30000))", 18_000_000),
        (strings, 6 * MIB),
    ] {
        let mut g: Grift = Grift::new().unwrap();
        let unlimited = g.evaluate(&mut (), program).unwrap();
        // Setting a limit asks for a collection as the next run begins: what is held after it
        // is what the interpreter keeps between runs.
        g.set_memory_limit(None);
        g.evaluate(&mut (), "1").unwrap();
        g.set_memory_limit(Some(g.memory_held() + room));
        for run in 0..50 {
            let limited = g.evaluate(&mut (), program);
            assert_eq!(
                limited,
                Ok(unlimited),
                "run {run} of {program} in {room} bytes"
            );
        }
    }
}

/// With no limit nothing is refused, and one set can be read and cleared.
#[test]
fn the_limit_is_off_until_set_and_off_again_when_cleared() {
    let mut g: Grift = Grift::new().unwrap();
    assert_eq!(g.memory_limit(), None);
    let megabyte = "x".repeat(MIB);
    let limit = g.memory_held() + MIB / 2;
    g.set_memory_limit(Some(limit));
    assert_eq!(g.memory_limit(), Some(limit));
    let e = g.string(&megabyte).unwrap_err();
    assert_eq!(e.grift_kind(), Some(Kind::OutOfMemory));
    g.set_memory_limit(None);
    assert_eq!(g.memory_limit(), None);
    assert!(g.string(&megabyte).is_ok());
    // The largest limit is a limit all the same, not none.
    g.set_memory_limit(Some(usize::MAX));
    assert_eq!(g.memory_limit(), Some(usize::MAX));
}

/// What `make` holds is counted, and so refused under a limit with less room than it needs, while
/// the same under a limit with room is made. The host makes it outside a run, and a program runs
/// after either way.
fn counted(what: &str, bytes: usize, make: impl Fn(&mut Grift) -> bool) {
    let mut g: Grift = Grift::new().unwrap();
    let before = g.memory_held();
    assert!(make(&mut g), "{what} with no limit");
    let grew = g.memory_held() - before;
    assert!(grew >= bytes, "{what} counted {grew} of {bytes} bytes");
    for (room, fits) in [(bytes / 2, false), (2 * grew + MIB, true)] {
        let mut g: Grift = Grift::new().unwrap();
        g.set_memory_limit(Some(g.memory_held() + room));
        assert_eq!(make(&mut g), fits, "{what} with {room} bytes of room");
        // Refused or not, outside a run: the interpreter runs a program after.
        let ran = g.evaluate(&mut (), "(length (range 0 100))");
        assert_eq!(ran, Ok(Value::Integer(100)), "after {what}");
    }
}

/// The limit counts the heap's chunks: a list of 100,000 pairs is 64 bytes a pair, less the
/// pairs that fit slots already free, under a quarter of them.
#[test]
fn the_limit_counts_the_chunks_of_slots() {
    let zeros = vec![Value::Integer(0); 100_000];
    counted("a list", 48 * zeros.len(), |g| g.list(&zeros).is_ok());
}

/// The limit counts a string's text, which lives outside its slot.
#[test]
fn the_limit_counts_a_string_s_text() {
    let text = "x".repeat(4 * MIB);
    counted("a string", text.len(), |g| g.string(&text).is_ok());
}

/// The limit counts a bytes value's bytes.
#[test]
fn the_limit_counts_a_bytes_value_s_bytes() {
    let bytes = vec![7; 4 * MIB];
    counted("a bytes value", bytes.len(), |g| g.bytes(&bytes).is_ok());
}

/// The limit counts a name's spelling.
#[test]
fn the_limit_counts_a_name_s_spelling() {
    let name = "n".repeat(4 * MIB);
    counted("a name", name.len(), |g| g.symbol(&name).is_ok());
}

/// The limit counts a context's bindings: 100,000 of them are 24 bytes each at least, besides
/// their names.
#[test]
fn the_limit_counts_a_context_s_bindings() {
    let names: Vec<String> = (0..100_000).map(|i| format!("n{i}")).collect();
    let spelled: usize = names.iter().map(|n| 2 * n.len() + 24 + 64).sum();
    counted("a context's bindings", spelled + 24 * names.len(), |g| {
        let record = g.context(None).unwrap();
        names.iter().all(|name| {
            let bound = g.symbol(name).map(Value::Symbol);
            bound.is_ok_and(|name| g.bind(&record, &name, Value::Inert).is_ok())
        })
    });
}

/// The limit counts the table of kept values: 24 bytes an entry.
#[test]
fn the_limit_counts_the_kept_table() {
    counted("kept values", 24 * 200_000, |g| {
        (0..200_000).all(|i| g.keep(Value::Integer(i)).is_ok())
    });
}

/// The limit counts the table of host functions: 16 bytes an entry, besides the slot each
/// function's value takes.
#[test]
fn the_limit_counts_the_host_functions_table() {
    counted("host functions", (64 + 16) * 100_000, |g| {
        (0..100_000).all(|_| {
            g.host_function("f", |_, _, _| Ok(Answer::Now(Value::Inert)))
                .is_ok()
        })
    });
}

/// Whatever a host makes, however large, the count never passes the limit: an object's bytes
/// outside its slot and the chunk its slot needs are checked together, and a table or a stack
/// is checked for the room it really takes as it grows.
#[test]
fn nothing_made_takes_the_count_past_the_limit() {
    let filled = |what: &str, make: &dyn Fn(&mut Grift, usize) -> bool| {
        let mut g: Grift = Grift::new().unwrap();
        let limit = g.memory_held() + 3 * MIB;
        g.set_memory_limit(Some(limit));
        let made = (0..10_000_000).take_while(|&i| {
            let made = make(&mut g, i);
            assert!(g.memory_held() <= limit, "{what} {i} passed the limit");
            made
        });
        assert!(made.count() < 10_000_000, "{what} was never refused");
    };
    for size in [1, 1000, 60_000, 65_536, 70_000, 200_000] {
        let text = "x".repeat(size);
        filled("a string", &|g, _| g.string(&text).is_ok());
    }
    // A string made when no slot is free needs a chunk as well as its text: pairs until a chunk
    // of 1,024 slots is added, the 1,023 left in it, and then a limit with room for either the
    // text or the chunk, not both.
    let mut g: Grift = Grift::new().unwrap();
    let held = g.memory_held();
    while g.memory_held() == held {
        g.pair(Value::Inert, Value::Inert).unwrap();
    }
    for _ in 0..1023 {
        g.pair(Value::Inert, Value::Inert).unwrap();
    }
    let limit = g.memory_held() + (64 << 10) + 30_000;
    g.set_memory_limit(Some(limit));
    let _ = g.string(&"x".repeat(60_000));
    assert!(g.memory_held() <= limit, "a string and its chunk");
    filled("a kept value", &|g, i| {
        g.keep(Value::Integer(i as i64)).is_ok()
    });
    filled("a pair", &|g, _| g.pair(Value::Inert, Value::Inert).is_ok());
    filled("a host function", &|g, _| {
        g.host_function("f", |_, _, _| Ok(Answer::Now(Value::Inert)))
            .is_ok()
    });
    filled("a binding", &|g, i| {
        let name = g.symbol(&format!("n{i}")).map(Value::Symbol);
        let top = g.top_level();
        name.is_ok_and(|name| g.bind(&top, &name, Value::Inert).is_ok())
    });
}

/// The limit counts the machine's stacks. A sum nested 50,000 deep makes no object as it is
/// evaluated, only frames and waiting arguments, so what is held at its innermost place, past
/// what was held as it began, is the stacks': 56 bytes a frame at least. It is read before the
/// limit is set, as the body of `deep`, so the limit's room is for evaluating it alone.
#[test]
fn the_limit_counts_the_machine_s_stacks() {
    const DEPTH: usize = 50_000;
    let nested = format!("{}(held){}", "(add 1 ".repeat(DEPTH), ")".repeat(DEPTH));
    let run = |room: Option<usize>| {
        let mut g: Grift<Vec<usize>> = Grift::new().unwrap();
        let held = g.host_function("held", |g, seen: &mut Vec<usize>, _| {
            seen.push(g.memory_held());
            Ok(Answer::Now(Value::Integer(0)))
        });
        g.define("held", held.unwrap()).unwrap();
        let mut seen = Vec::new();
        let deep = format!("(define! deep (lambda! () {nested}))");
        g.evaluate(&mut seen, &deep).unwrap();
        // Setting a limit asks for a collection as the next run begins: the room is past what
        // is live after it.
        g.set_memory_limit(None);
        g.evaluate(&mut seen, "1").unwrap();
        g.set_memory_limit(room.map(|room| g.memory_held() + room));
        let done = g.evaluate(&mut seen, "(held) (deep)");
        (done.map_err(|e| e.grift_kind()), seen)
    };
    let (done, seen) = run(None);
    assert_eq!(done, Ok(Value::Integer(DEPTH as i64)));
    let stacks = seen[1] - seen[0];
    assert!(stacks >= 56 * DEPTH, "the stacks counted {stacks} bytes");
    let (done, seen) = run(Some(56 * DEPTH / 2));
    assert_eq!((done, seen.len()), (Err(Some(Kind::OutOfMemory)), 1));
}
