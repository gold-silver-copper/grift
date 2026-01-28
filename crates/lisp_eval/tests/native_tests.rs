use lisp_eval::native::*;
use lisp_eval::{Lisp, Evaluator, register_native};
use pwn_arena::{ArenaIndex, ArenaResult};

#[test]
fn test_from_lisp_i64() {
    let lisp: Lisp<100> = Lisp::new();
    let num = lisp.number(42).unwrap();
    assert_eq!(i64::from_lisp(&lisp, num).unwrap(), 42);
}

#[test]
fn test_to_lisp_i64() {
    let lisp: Lisp<100> = Lisp::new();
    let idx = 42i64.to_lisp(&lisp).unwrap();
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
    
    let (v1, rest1) = extract_arg::<100, i64>(&lisp, l1).unwrap();
    assert_eq!(v1, 1);
    
    let (v2, rest2) = extract_arg::<100, i64>(&lisp, rest1).unwrap();
    assert_eq!(v2, 2);
    
    let (v3, rest3) = extract_arg::<100, i64>(&lisp, rest2).unwrap();
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

// Test the register_native! macro
#[test]
fn test_register_native_macro() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Register functions using the new macro syntax
    register_native!(eval, "add-one", (x: i64) -> i64 {
        x + 1
    }).unwrap();
    
    register_native!(eval, "add", (a: i64, b: i64) -> i64 {
        a + b
    }).unwrap();
    
    register_native!(eval, "const-42", () -> i64 {
        42
    }).unwrap();
    
    // Test no-arg function
    let result = eval.eval_str("(const-42)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // Test single-arg function
    let result = eval.eval_str("(add-one 5)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(6));
    
    // Test two-arg function
    let result = eval.eval_str("(add 3 4)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(7));
}

// Test register_native! with three arguments
#[test]
fn test_register_native_three_args() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    register_native!(eval, "clamp", (value: i64, min_val: i64, max_val: i64) -> i64 {
        if value < min_val {
            min_val
        } else if value > max_val {
            max_val
        } else {
            value
        }
    }).unwrap();
    
    // Test clamping below minimum
    let result = eval.eval_str("(clamp -10 0 100)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0));
    
    // Test clamping above maximum
    let result = eval.eval_str("(clamp 150 0 100)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(100));
    
    // Test value within range
    let result = eval.eval_str("(clamp 50 0 100)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(50));
}

// Test register_native! with boolean return
#[test]
fn test_register_native_bool_return() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    register_native!(eval, "is-even", (x: i64) -> bool {
        x % 2 == 0
    }).unwrap();
    
    let result = eval.eval_str("(is-even 4)").unwrap();
    assert!(lisp.get(result).unwrap().is_true());
    
    let result = eval.eval_str("(is-even 7)").unwrap();
    assert!(lisp.get(result).unwrap().is_false());
}

// Test register_native! with four arguments
#[test]
fn test_register_native_four_args() {
    let lisp: Lisp<1000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    register_native!(eval, "weighted-avg", (a: i64, b: i64, wa: i64, wb: i64) -> i64 {
        (a * wa + b * wb) / (wa + wb)
    }).unwrap();
    
    // Weighted average of 10 (weight 3) and 20 (weight 1) = (30 + 20) / 4 = 12
    let result = eval.eval_str("(weighted-avg 10 20 3 1)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(12));
}
