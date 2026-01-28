//! Tests for Lisp-Rust interop (lisp_fn! macro and FromLisp/ToLisp traits)

use lisp_eval::native::*;
use lisp_eval::{Lisp, lisp_fn};
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

// Test the lisp_fn! macro
lisp_fn!(fn_add_one, (x: isize) -> isize, { x + 1 });
lisp_fn!(fn_add, (a: isize, b: isize) -> isize, { a + b });
lisp_fn!(fn_const, () -> isize, { 42 });

// Test the @with_lisp variant - the body can access lisp and remaining args
lisp_fn!(fn_bit_check @with_lisp, (value: isize, bit: isize) -> bool, {
    if bit >= 0 && bit < 64 {
        (value & (1isize << bit)) != 0
    } else {
        false
    }
});

// Test three-arg @with_lisp variant
lisp_fn!(fn_clamp @with_lisp, (value: isize, min: isize, max: isize) -> isize, {
    if value < min { min } else if value > max { max } else { value }
});

#[test]
fn test_lisp_fn_macro() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Test no-arg function
    let nil = lisp.nil().unwrap();
    let result = fn_const(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // Test single-arg function
    let n5 = lisp.number(5).unwrap();
    let args = lisp.cons(n5, nil).unwrap();
    let result = fn_add_one(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6));
    
    // Test two-arg function
    let n3 = lisp.number(3).unwrap();
    let n4 = lisp.number(4).unwrap();
    let args = lisp.cons(n4, nil).unwrap();
    let args = lisp.cons(n3, args).unwrap();
    let result = fn_add(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(7));
}

#[test]
fn test_lisp_fn_with_lisp() {
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Test @with_lisp two-arg function (bit_check)
    let value = lisp.number(0b1010).unwrap();
    let bit1 = lisp.number(1).unwrap();
    let bit3 = lisp.number(3).unwrap();
    
    // Check bit 1: 0b1010 has bit 1 set
    let args = lisp.cons(bit1, nil).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = fn_bit_check(&lisp, args).unwrap();
    assert!(lisp.get(result).unwrap().is_true());
    
    // Check bit 3: 0b1010 has bit 3 set
    let args = lisp.cons(bit3, nil).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = fn_bit_check(&lisp, args).unwrap();
    assert!(lisp.get(result).unwrap().is_true());
    
    // Check bit 0: 0b1010 does not have bit 0 set
    let bit0 = lisp.number(0).unwrap();
    let args = lisp.cons(bit0, nil).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = fn_bit_check(&lisp, args).unwrap();
    assert!(lisp.get(result).unwrap().is_false());
}

#[test]
fn test_lisp_fn_with_lisp_three_args() {
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
    let result = fn_clamp(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(10));
    
    // 5 clamped to [0, 10] should be 5
    let value = lisp.number(5).unwrap();
    let args = lisp.cons(max, nil).unwrap();
    let args = lisp.cons(min, args).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = fn_clamp(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(5));
    
    // -5 clamped to [0, 10] should be 0
    let value = lisp.number(-5).unwrap();
    let args = lisp.cons(max, nil).unwrap();
    let args = lisp.cons(min, args).unwrap();
    let args = lisp.cons(value, args).unwrap();
    let result = fn_clamp(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0));
}

// ============================================================================
// Tests for lisp_fn! macro with stateful functions
// ============================================================================

use core::sync::atomic::{AtomicUsize, Ordering};

// Separate static variables for each test to avoid race conditions
static COUNTER_NO_ARGS: AtomicUsize = AtomicUsize::new(0);
static COUNTER_ONE_ARG: AtomicUsize = AtomicUsize::new(0);
static COUNTER_TWO_ARGS: AtomicUsize = AtomicUsize::new(0);

// Test zero-argument stateful function
lisp_fn!(
    fn_increment_counter,
    () -> isize,
    {
        COUNTER_NO_ARGS.fetch_add(1, Ordering::Relaxed) as isize
    }
);

// Test single-argument stateful function
lisp_fn!(
    fn_add_to_counter,
    (n: isize) -> isize,
    {
        COUNTER_ONE_ARG.fetch_add(n as usize, Ordering::Relaxed) as isize
    }
);

// Test two-argument stateful function
lisp_fn!(
    fn_set_counter_if_less,
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

#[test]
fn test_lisp_fn_stateful_no_args() {
    // Reset counter for this test
    COUNTER_NO_ARGS.store(0, Ordering::Relaxed);
    
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // First increment
    let result = fn_increment_counter(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0)); // returns old value
    
    // Second increment
    let result = fn_increment_counter(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(1));
    
    // Third increment
    let result = fn_increment_counter(&lisp, nil).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(2));
}

#[test]
fn test_lisp_fn_stateful_one_arg() {
    // Reset counter for this test
    COUNTER_ONE_ARG.store(0, Ordering::Relaxed);
    
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Add 5 to counter
    let n5 = lisp.number(5).unwrap();
    let args = lisp.cons(n5, nil).unwrap();
    let result = fn_add_to_counter(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0)); // returns old value
    
    // Counter should now be 5, add 10 more
    let n10 = lisp.number(10).unwrap();
    let args = lisp.cons(n10, nil).unwrap();
    let result = fn_add_to_counter(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(5)); // returns old value
    
    // Verify counter is now 15
    assert_eq!(COUNTER_ONE_ARG.load(Ordering::Relaxed), 15);
}

#[test]
fn test_lisp_fn_stateful_two_args() {
    // Reset counter for this test
    COUNTER_TWO_ARGS.store(5, Ordering::Relaxed);
    
    let lisp: Lisp<100> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Set to 100 if counter < 10 (should set)
    let threshold = lisp.number(10).unwrap();
    let new_val = lisp.number(100).unwrap();
    let args = lisp.cons(new_val, nil).unwrap();
    let args = lisp.cons(threshold, args).unwrap();
    let result = fn_set_counter_if_less(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(100));
    assert_eq!(COUNTER_TWO_ARGS.load(Ordering::Relaxed), 100);
    
    // Set to 50 if counter < 10 (should NOT set)
    let new_val = lisp.number(50).unwrap();
    let args = lisp.cons(new_val, nil).unwrap();
    let args = lisp.cons(threshold, args).unwrap();
    let result = fn_set_counter_if_less(&lisp, args).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(100)); // returns current value
    assert_eq!(COUNTER_TWO_ARGS.load(Ordering::Relaxed), 100); // unchanged
}

// ============================================================================
// Tests for embedded builtins (now part of core)
// ============================================================================

use lisp_eval::Evaluator;

#[test]
fn test_embedded_peek_poke() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test poke and peek
    let result = eval.eval_str("(poke 0 42)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    let result = eval.eval_str("(peek 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
}

#[test]
fn test_embedded_peek32_poke32() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test poke32 and peek32
    let result = eval.eval_str("(poke32 0 305419896)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(305419896));
    
    let result = eval.eval_str("(peek32 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(305419896));
}

#[test]
fn test_embedded_gpio() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test gpio-write and gpio-read
    let result = eval.eval_str("(gpio-write 0 255)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(255));
    
    let result = eval.eval_str("(gpio-read 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(255));
    
    // Test gpio-set
    let _result = eval.eval_str("(gpio-write 1 0)").unwrap();
    let result = eval.eval_str("(gpio-set 1 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(1)); // bit 0 set
    
    // Test gpio-clear  
    let result = eval.eval_str("(gpio-clear 1 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0)); // bit 0 cleared
    
    // Test gpio-toggle
    let result = eval.eval_str("(gpio-toggle 1 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(1)); // bit 0 toggled on
}

#[test]
fn test_embedded_bit_operations() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Test bit-set?
    let result = eval.eval_str("(bit-set? 5 0)").unwrap();
    assert!(lisp.get(result).unwrap().is_true()); // 5 = 0b101, bit 0 is set
    
    let result = eval.eval_str("(bit-set? 5 1)").unwrap();
    assert!(lisp.get(result).unwrap().is_false()); // 5 = 0b101, bit 1 is not set
    
    // Test bit-extract
    let result = eval.eval_str("(bit-extract 255 4 4)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(15)); // Extract high nibble
    
    // Test bit-insert
    let result = eval.eval_str("(bit-insert 0 15 4 4)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(240)); // Insert 0xF at position 4
}
