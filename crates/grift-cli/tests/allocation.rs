// This embedding test uses a budget allocator in its own binary: the core forbids unsafe code.
use grift::{Grift, Value};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // Other test-harness threads keep their ordinary allocator. Freed bytes restore the budget,
    // including storage allocated before the limit was set, just as a host's memory budget does.
    static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
}

struct Budget;

unsafe impl GlobalAlloc for Budget {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let allowed = REMAINING
            .try_with(|left| left.get().is_none_or(|n| layout.size() <= n))
            .unwrap_or(true);
        if !allowed {
            return std::ptr::null_mut();
        }
        // SAFETY: forward the caller's valid allocation layout to the system allocator.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let _ = REMAINING.try_with(|left| {
                left.set(left.get().map(|n| n - layout.size()));
            });
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        let _ = REMAINING.try_with(|left| {
            left.set(left.get().map(|n| n.saturating_add(layout.size())));
        });
        // SAFETY: the pointer and its original layout came from this system allocator.
        unsafe { System.dealloc(p, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: Budget = Budget;

struct Limited;
impl Drop for Limited {
    fn drop(&mut self) {
        REMAINING.set(None);
    }
}

#[test]
fn a_nonempty_call_collects_before_allocating_and_keeps_all_its_inputs() {
    let text = "x".repeat(1_000_000);
    let mut other: Grift = Grift::new().unwrap();
    let foreign = other.string("foreign").unwrap();
    for references in [false, true] {
        let mut g: Grift = Grift::new().unwrap();
        let f = g.evaluate(&mut (), "(lambda! (x) x)").unwrap();
        let kept = g.keep(f).unwrap();
        let stale = g.string("discarded").unwrap();
        // Warm the collector and leave enough reusable slots for a call after collection.
        g.evaluate(&mut (), "(length (map list (range 0 5000)))")
            .unwrap();
        let f = g.kept(&kept).unwrap();
        g.call(&mut (), &f, &[Value::Integer(3)]).unwrap();
        let argument = if references {
            // Every heap-reference kind, with both the function and its argument unkept.
            g.evaluate(
                &mut (),
                "(list (pair 1 2) \"live\" #bytes(3)
                (context) (vau! () #ignore #inert) (lambda! () #inert))",
            )
            .unwrap()
        } else {
            Value::Integer(3)
        };
        if references {
            g.release(kept).unwrap();
        }
        let mut before = String::new();
        g.write(&argument, &mut before).unwrap();
        REMAINING.set(Some(text.len()));
        let limit = Limited;
        let garbage = g.string(&text).unwrap();
        let exhausted = g.string("no space").unwrap_err();
        let bad_foreign = g.call(&mut (), &f, std::slice::from_ref(&foreign));
        let bad_stale = g.call(&mut (), &f, std::slice::from_ref(&stale));
        let uncollected = g.as_str(&garbage).is_ok();
        let result = g.call(&mut (), &f, std::slice::from_ref(&argument));
        drop(limit); // Restore allocation before assertions can format a failure.
        assert_eq!(g.is_kind(&exhausted, "out-of-memory"), Ok(true));
        assert_eq!(result, Ok(argument.clone()), "references={references}");
        assert_eq!(
            g.is_kind(&bad_foreign.unwrap_err(), "foreign-value"),
            Ok(true)
        );
        assert_eq!(g.is_kind(&bad_stale.unwrap_err(), "stale-value"), Ok(true));
        assert!(
            uncollected,
            "invalid input must be rejected before collection"
        );
        let mut after = String::new();
        g.write(&argument, &mut after).unwrap();
        assert_eq!(after, before);
        assert_eq!(
            g.is_kind(&g.as_str(&garbage).unwrap_err(), "stale-value"),
            Ok(true)
        );
    }
}
