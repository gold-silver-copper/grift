//! What a program's `out-of-memory` report needs, when the allocator refuses for real. A budget
//! on this thread's allocations stands in for the operating system's limit, at the same point in
//! a program every time; it lives in this package because the core forbids `unsafe`.

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

/// However little the last refusal left, the report says the kind and all three places. The
/// budgets are spread so that many run out on a small object with almost nothing left over.
#[test]
fn an_out_of_memory_report_is_whole_with_nothing_left_to_allocate() {
    let mut short = Vec::new();
    for budget in (0..150).map(|k| (1 << 18) + k * 53003) {
        let mut g: Grift = Grift::new().unwrap();
        let mut text = String::with_capacity(4096);
        LIMIT.set(LIVE.get() + budget);
        let done = g.evaluate(&mut (), "(define! f (lambda! (n) (add 1 (f n))))\n(f 1)");
        let written = done.as_ref().err().map(|e| g.describe(e, &mut text));
        LIMIT.set(usize::MAX);
        assert!(written.is_some(), "the program needs more than {budget}");
        if text != "out-of-memory in (f n)\n  from (add 1 (f n))\n  from (f 1)" {
            short.push((budget, text));
        }
    }
    assert!(
        short.is_empty(),
        "{} short: {:?}",
        short.len(),
        &short[..short.len().min(6)]
    );
}
