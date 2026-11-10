//! What a program's `out-of-memory` report needs, when the allocator refuses for real. A budget
//! on this thread's allocations stands in for the operating system's limit, at the same point in
//! a program every time; it lives in this package because the core forbids `unsafe`.
#![allow(unsafe_code, clippy::unwrap_used, clippy::indexing_slicing)]

use grift::Grift;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static LIMIT: Cell<usize> = const { Cell::new(usize::MAX) };
}

struct Budgeted;

/// Whether this thread may hold `after` bytes, which it then does.
fn allowed(after: usize) -> bool {
    let ok = after <= LIMIT.get();
    if ok {
        LIVE.set(after);
    }
    ok
}

// SAFETY: every call goes to the system allocator with the arguments it was given, or is refused
// with a null pointer, which `GlobalAlloc` allows.
unsafe impl GlobalAlloc for Budgeted {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if allowed(LIVE.get().saturating_add(l.size())) {
            unsafe { System.alloc(l) }
        } else {
            std::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.set(LIVE.get().saturating_sub(l.size()));
        unsafe { System.dealloc(p, l) }
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        if allowed(LIVE.get().saturating_sub(l.size()).saturating_add(new)) {
            unsafe { System.realloc(p, l, new) }
        } else {
            std::ptr::null_mut()
        }
    }
}

#[global_allocator]
static ALLOCATOR: Budgeted = Budgeted;

/// However little the last refusal left, the ending keeps every place of its trace, 3 or 302,
/// and the report says the kind and all three. The budgets are spread so that many run out on a
/// small object with almost nothing left over.
#[test]
fn an_out_of_memory_report_is_whole_with_nothing_left_to_allocate() {
    let f = "(define! f (lambda! (n) (add 1 (f n))))\n";
    let deep = format!("{f}{}(f 1){}", "(add 1 ".repeat(300), ")".repeat(300));
    let mut short = Vec::new();
    for (program, places) in [(format!("{f}(f 1)"), 3), (deep, 302)] {
        for budget in (0..150).map(|k| (1 << 18) + k * 53003) {
            let mut g: Grift = Grift::new().unwrap();
            let mut text = String::with_capacity(4096);
            LIMIT.set(LIVE.get() + budget);
            let done = g.evaluate(&mut (), &program);
            let written = done.as_ref().err().map(|e| g.describe(e, &mut text, 4096));
            LIMIT.set(usize::MAX);
            assert!(written.is_some(), "the program needs more than {budget}");
            let traced = done.map_or_else(|e| e.trace.len(), |_| 0);
            if traced != places
                || places == 3 && text != "out-of-memory in (f n)\n  from (add 1 ...)\n  from (f 1)"
            {
                short.push((budget, traced));
            }
        }
    }
    assert!(
        short.is_empty(),
        "{} short: {:?}",
        short.len(),
        &short[..short.len().min(6)]
    );
}

/// A report of an out-of-memory ending needs memory for its own text and the printer's work, not
/// for the program's trace or for a context in it: a 4 KiB report has 16 KiB to spare, at a depth
/// of 2,000 and with a context of 10,000 names in the trace.
#[test]
fn an_out_of_memory_report_needs_memory_for_its_text_alone() {
    let hog = "(define! hog (lambda! (xs n) (hog (pair n xs) (add n 1))))\n";
    let deep = format!(
        "{hog}{}(hog () 0){}",
        "(add 1 ".repeat(2000),
        ")".repeat(2000)
    );
    let context = format!(
        "(define! c (context (current-context)))
         (define! fill (lambda! (i) (if! (less-than? i 10000)
           (sequence! (bind c (string-to-symbol (integer-to-string i)) 0) (fill (add i 1))) #inert)))
         (fill 0)
         {hog}(evaluate (list add c (list hog () 0)) (current-context))"
    );
    for program in [deep, context] {
        let mut g: Grift = Grift::new().unwrap();
        LIMIT.set(LIVE.get() + (32 << 20));
        let e = g.evaluate(&mut (), &program).unwrap_err();
        LIMIT.set(usize::MAX);
        let mut text = grift::TextBuffer::new();
        LIMIT.set(LIVE.get() + (16 << 10));
        let written = g.describe(&e, &mut text, 4096);
        LIMIT.set(usize::MAX);
        let why = written
            .as_ref()
            .err()
            .map(|w| g.symbol_name(&w.kind).map(String::from));
        assert!(
            written.is_ok() && text.as_str().starts_with("out-of-memory in ("),
            "{why:?} {:?}",
            &text.as_str()[..text.as_str().len().min(80)]
        );
    }
}

/// A combination in the trace that nests 5,000 deep through its first element, as quoted data,
/// is cut by the report: its report needs memory for the budget, not for the program's data.
#[test]
fn an_out_of_memory_report_of_deep_quoted_data_needs_no_more_than_its_budget() {
    let hog = "(define! hog (lambda! (xs n) (hog (pair n xs) (add n 1))))\n";
    let program = format!(
        "{hog}(list (quote! {}x{}) (hog () 0))",
        "(".repeat(5000),
        ")".repeat(5000)
    );
    for spare in [16 << 10, 64 << 10] {
        let mut g: Grift = Grift::new().unwrap();
        LIMIT.set(LIVE.get() + (32 << 20));
        let e = g.evaluate(&mut (), &program).unwrap_err();
        LIMIT.set(usize::MAX);
        let mut text = grift::TextBuffer::new();
        LIMIT.set(LIVE.get() + spare);
        let written = g.describe(&e, &mut text, 4096);
        LIMIT.set(usize::MAX);
        let why = written
            .as_ref()
            .err()
            .map(|w| g.symbol_name(&w.kind).map(String::from));
        assert!(
            written.is_ok() && text.as_str().starts_with("out-of-memory in ("),
            "{spare}: {why:?}"
        );
    }
}

/// A host's call collects before it copies its arguments: with the budget spent on garbage, a call
/// with an argument still runs.
#[test]
fn a_call_collects_before_copying_its_arguments() {
    let mut g: Grift = Grift::new().unwrap();
    let f = g.evaluate(&mut (), "(lambda! (x) x)").unwrap();
    LIMIT.set(LIVE.get() + (2 << 20));
    let _garbage = g.string(&"x".repeat(1 << 20)).unwrap();
    LIMIT.set(LIVE.get());
    assert!(g.string("refused").is_err());
    let called = g.call(&mut (), &f, &[grift::Value::Integer(3)]);
    LIMIT.set(usize::MAX);
    assert_eq!(called, Ok(grift::Value::Integer(3)));
}
