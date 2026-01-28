use lisp_eval::native::*;
use lisp_eval::{Lisp, define_native};
use pwn_arena::{ArenaIndex, ArenaResult};

#[test]
fn test_from_lisp_isize() {
    let lisp: Lisp<100> = Lisp::new();
    let num = lisp.number(42).unwrap();
    assert_eq!(isize::from_lisp(&lisp, num).unwrap(), 42);
}

#[test]
fn test_to_lisp_isize() {
    let lisp: Lisp<100> = Lisp::new();
    let idx = 42isize.to_lisp(&lisp).unwrap();
    assert_eq!(lisp.get(idx).unwrap().as_number(), Some(42));
}

#[test]
fn test_from_lisp_bool() {
    let lisp: Lisp<100> = Lisp::new();
    let t = lisp.true_val().unwrap();
    let f = lisp.false_val().unwrap();
    assert!(bool::from_lisp(&lisp, t).unwrap());
    assert!(!bool::from_lisp(&lisp, f).unwrap());
}

#[test]
fn test_to_lisp_bool() {
    let lisp: Lisp<100> = Lisp::new();
    let t_idx = true.to_lisp(&lisp).unwrap();
    let f_idx = false.to_lisp(&lisp).unwrap();
    assert!(lisp.get(t_idx).unwrap().is_true());
    assert!(lisp.get(f_idx).unwrap().is_false());
}

#[test]
fn test_native_registry() {
    fn dummy_fn<const N: usize>(lisp: &Lisp<N>, _args: ArenaIndex) -> ArenaResult<ArenaIndex> {
        lisp.nil()
    }

    let mut registry: NativeRegistry<100> = NativeRegistry::new();
    assert!(registry.is_empty());
    
    registry.register("dummy", dummy_fn);
    assert_eq!(registry.len(), 1);
    assert!(registry.lookup("dummy").is_some());
    assert!(registry.lookup("nonexistent").is_none());
}

#[test]
fn test_extract_arg() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Build list (1 2 3)
    let nil = lisp.nil().unwrap();
    let n3 = lisp.number(3).unwrap();
    let l3 = lisp.cons(n3, nil).unwrap();
    let n2 = lisp.number(2).unwrap();
    let l2 = lisp.cons(n2, l3).unwrap();
    let n1 = lisp.number(1).unwrap();
    let l1 = lisp.cons(n1, l2).unwrap();
    
    let (v1, rest1) = extract_arg::<100, isize>(&lisp, l1).unwrap();
    assert_eq!(v1, 1);
    
    let (v2, rest2) = extract_arg::<100, isize>(&lisp, rest1).unwrap();
    assert_eq!(v2, 2);
    
    let (v3, rest3) = extract_arg::<100, isize>(&lisp, rest2).unwrap();
    assert_eq!(v3, 3);
    
    assert!(args_empty(&lisp, rest3).unwrap());
}

#[test]
fn test_count_args() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Empty list
    let nil = lisp.nil().unwrap();
    assert_eq!(count_args(&lisp, nil).unwrap(), 0);
    
    // Single element
    let n1 = lisp.number(1).unwrap();
    let l1 = lisp.cons(n1, nil).unwrap();
    assert_eq!(count_args(&lisp, l1).unwrap(), 1);
    
    // Three elements
    let n2 = lisp.number(2).unwrap();
    let n3 = lisp.number(3).unwrap();
    let l3 = lisp.cons(n3, nil).unwrap();
    let l2 = lisp.cons(n2, l3).unwrap();
    let l1 = lisp.cons(n1, l2).unwrap();
    assert_eq!(count_args(&lisp, l1).unwrap(), 3);
}

// Test the define_native! macro
define_native!(native_add_one, (x: isize) -> isize, { x + 1 });
define_native!(native_add, (a: isize, b: isize) -> isize, { a + b });
define_native!(native_const, () -> isize, { 42 });

// Test the @with_lisp variant - the body can access lisp and remaining args
define_native!(native_bit_check @with_lisp, (value: isize, bit: isize) -> bool, {
    if bit >= 0 && bit < 64 {
        (value & (1isize << bit)) != 0
    } else {
        false
    }
});

// Test three-arg @with_lisp variant
define_native!(native_clamp @with_lisp, (value: isize, min: isize, max: isize) -> isize, {
    if value < min { min } else if value > max { max } else { value }
});

#[test]
fn test_define_native_macro() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Test no-arg function
    let nil = lisp.nil().unwrap();
    let result = native_const(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // Test single-arg function
    let n5 = lisp.number(5).unwrap();
    let args = lisp.cons(n5, nil).unwrap();
    let result = native_add_one(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6));
    
    // Test two-arg function
    let n3 = lisp.number(3).unwrap();
    let n4 = lisp.number(4).unwrap();
    let args = lisp.cons(n4, nil).unwrap();
    let args = lisp.cons(n3, args).unwrap();
    let result = native_add(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(7));
}

#[test]
fn test_define_native_with_lisp() {
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Test @with_lisp two-arg function (bit_check)
    let value = lisp.number(0b1010).unwrap();
    let bit1 = lisp.number(1).unwrap();
    let bit3 = lisp.number(3).unwrap();
    
    // Check bit 1: 0b1010 has bit 1 set
    let args = lisp.cons(bit1, nil).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = native_bit_check(&lisp, args).unwrap();
    assert!(lisp.get(result).unwrap().is_true());
    
    // Check bit 3: 0b1010 has bit 3 set
    let args = lisp.cons(bit3, nil).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = native_bit_check(&lisp, args).unwrap();
    assert!(lisp.get(result).unwrap().is_true());
    
    // Check bit 0: 0b1010 does not have bit 0 set
    let bit0 = lisp.number(0).unwrap();
    let args = lisp.cons(bit0, nil).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = native_bit_check(&lisp, args).unwrap();
    assert!(lisp.get(result).unwrap().is_false());
}

#[test]
fn test_define_native_with_lisp_three_args() {
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Test @with_lisp three-arg function (clamp)
    let value = lisp.number(15).unwrap();
    let min = lisp.number(0).unwrap();
    let max = lisp.number(10).unwrap();
    
    // 15 clamped to [0, 10] should be 10
    let args = lisp.cons(max, nil).unwrap();
    let args = lisp.cons(min, args).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = native_clamp(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(10));
    
    // 5 clamped to [0, 10] should be 5
    let value = lisp.number(5).unwrap();
    let args = lisp.cons(max, nil).unwrap();
    let args = lisp.cons(min, args).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = native_clamp(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(5));
    
    // -5 clamped to [0, 10] should be 0
    let value = lisp.number(-5).unwrap();
    let args = lisp.cons(max, nil).unwrap();
    let args = lisp.cons(min, args).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = native_clamp(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0));
}

// ============================================================================
// Tests for define_native_stateful! macro
// ============================================================================

use core::sync::atomic::{AtomicUsize, Ordering};
use lisp_eval::define_native_stateful;

// Separate static variables for each test to avoid race conditions
static COUNTER_NO_ARGS: AtomicUsize = AtomicUsize::new(0);
static COUNTER_ONE_ARG: AtomicUsize = AtomicUsize::new(0);
static COUNTER_TWO_ARGS: AtomicUsize = AtomicUsize::new(0);
static COUNTER_EVALUATOR: AtomicUsize = AtomicUsize::new(0);

// Test zero-argument stateful function
define_native_stateful!(
    native_increment_counter,
    static: COUNTER_NO_ARGS,
    () -> isize,
    {
        COUNTER_NO_ARGS.fetch_add(1, Ordering::Relaxed) as isize
    }
);

// Test single-argument stateful function
define_native_stateful!(
    native_add_to_counter,
    static: COUNTER_ONE_ARG,
    (n: isize) -> isize,
    {
        COUNTER_ONE_ARG.fetch_add(n as usize, Ordering::Relaxed) as isize
    }
);

// Test two-argument stateful function
define_native_stateful!(
    native_set_counter_if_less,
    static: COUNTER_TWO_ARGS,
    (threshold: isize, new_val: isize) -> isize,
    {
        let current = COUNTER_TWO_ARGS.load(Ordering::Relaxed) as isize;
        if current < threshold {
            COUNTER_TWO_ARGS.store(new_val as usize, Ordering::Relaxed);
            new_val
        } else {
            current
        }
    }
);

// Functions for evaluator test
define_native_stateful!(
    native_eval_inc,
    static: COUNTER_EVALUATOR,
    () -> isize,
    {
        COUNTER_EVALUATOR.fetch_add(1, Ordering::Relaxed) as isize
    }
);

define_native_stateful!(
    native_eval_add,
    static: COUNTER_EVALUATOR,
    (n: isize) -> isize,
    {
        COUNTER_EVALUATOR.fetch_add(n as usize, Ordering::Relaxed) as isize
    }
);

#[test]
fn test_define_native_stateful_no_args() {
    // Reset counter for this test
    COUNTER_NO_ARGS.store(0, Ordering::Relaxed);
    
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // First increment
    let result = native_increment_counter(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0)); // returns old value
    
    // Second increment
    let result = native_increment_counter(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(1));
    
    // Third increment
    let result = native_increment_counter(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(2));
}

#[test]
fn test_define_native_stateful_one_arg() {
    // Reset counter for this test
    COUNTER_ONE_ARG.store(0, Ordering::Relaxed);
    
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Add 5 to counter
    let n5 = lisp.number(5).unwrap();
    let args = lisp.cons(n5, nil).unwrap();
    let result = native_add_to_counter(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0)); // returns old value
    
    // Counter should now be 5, add 10 more
    let n10 = lisp.number(10).unwrap();
    let args = lisp.cons(n10, nil).unwrap();
    let result = native_add_to_counter(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(5)); // returns old value
    
    // Verify counter is now 15
    assert_eq!(COUNTER_ONE_ARG.load(Ordering::Relaxed), 15);
}

#[test]
fn test_define_native_stateful_two_args() {
    // Reset counter for this test
    COUNTER_TWO_ARGS.store(5, Ordering::Relaxed);
    
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Set to 100 if counter < 10 (should set)
    let threshold = lisp.number(10).unwrap();
    let new_val = lisp.number(100).unwrap();
    let args = lisp.cons(new_val, nil).unwrap();
    let args = lisp.cons(threshold, args).unwrap();
    let result = native_set_counter_if_less(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(100));
    assert_eq!(COUNTER_TWO_ARGS.load(Ordering::Relaxed), 100);
    
    // Set to 50 if counter < 10 (should NOT set)
    let new_val = lisp.number(50).unwrap();
    let args = lisp.cons(new_val, nil).unwrap();
    let args = lisp.cons(threshold, args).unwrap();
    let result = native_set_counter_if_less(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(100)); // returns current value
    assert_eq!(COUNTER_TWO_ARGS.load(Ordering::Relaxed), 100); // unchanged
}

#[test]
fn test_define_native_stateful_with_evaluator() {
    // Reset counter for this test
    COUNTER_EVALUATOR.store(0, Ordering::Relaxed);
    
    use lisp_eval::Evaluator;
    
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Register the stateful native functions
    eval.register_native("inc-counter", native_eval_inc).unwrap();
    eval.register_native("add-counter", native_eval_add).unwrap();
    
    // Test calling the functions from Lisp
    let result = eval.eval_str("(inc-counter)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0));
    
    let result = eval.eval_str("(inc-counter)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(1));
    
    let result = eval.eval_str("(add-counter 10)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(2));
    
    // Verify counter is 12 now
    assert_eq!(COUNTER_EVALUATOR.load(Ordering::Relaxed), 12);
}
