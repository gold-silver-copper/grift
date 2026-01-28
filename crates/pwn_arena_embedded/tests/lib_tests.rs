use pwn_arena_embedded::*;
use lisp_eval::{Lisp, Evaluator};

#[test]
fn test_peek_poke() {
    reset_mock_hardware();
    
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    register_embedded_natives(&mut eval).unwrap();
    
    // Write a value
    let result = eval.eval_str("(poke 0 42)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
    
    // Read it back
    let result = eval.eval_str("(peek 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(42));
}

#[test]
fn test_peek_poke_32() {
    reset_mock_hardware();
    
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    register_embedded_natives(&mut eval).unwrap();
    
    // Write a 32-bit value (305419896 = 0x12345678)
    let result = eval.eval_str("(poke32 100 305419896)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0x12345678));
    
    // Read it back
    let result = eval.eval_str("(peek32 100)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0x12345678));
}

#[test]
fn test_gpio_read_write() {
    reset_mock_hardware();
    
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    register_embedded_natives(&mut eval).unwrap();
    
    // Write to GPIO register 0
    let result = eval.eval_str("(gpio-write 0 255)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(255));
    
    // Read it back
    let result = eval.eval_str("(gpio-read 0)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(255));
}

#[test]
fn test_gpio_bit_operations() {
    reset_mock_hardware();
    
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    register_embedded_natives(&mut eval).unwrap();
    
    // Use GPIO register 1 instead of 0 to avoid conflicts with test_gpio_read_write
    // which uses register 0. This prevents race conditions when tests run in parallel.
    let gpio_reg = 1;
    
    // Explicitly clear GPIO register to ensure clean state
    let _ = eval.eval_str(&format!("(gpio-write {} 0)", gpio_reg)).unwrap();
    
    // Verify GPIO register is 0 after reset
    let result = eval.eval_str(&format!("(gpio-read {})", gpio_reg)).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0), "GPIO register should be 0 after reset");
    
    // Set bit 0
    let result = eval.eval_str(&format!("(gpio-set {} 0)", gpio_reg)).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(1));
    
    // Set bit 3
    let result = eval.eval_str(&format!("(gpio-set {} 3)", gpio_reg)).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(9)); // 1 + 8
    
    // Toggle bit 0
    let result = eval.eval_str(&format!("(gpio-toggle {} 0)", gpio_reg)).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(8));
    
    // Clear bit 3
    let result = eval.eval_str(&format!("(gpio-clear {} 3)", gpio_reg)).unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(0));
}

#[test]
fn test_bit_set() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    register_embedded_natives(&mut eval).unwrap();
    
    // Check bit 0 of 1
    let result = eval.eval_str("(bit-set? 1 0)").unwrap();
    assert!(lisp.get(result).unwrap().is_true());
    
    // Check bit 1 of 1
    let result = eval.eval_str("(bit-set? 1 1)").unwrap();
    assert!(lisp.get(result).unwrap().is_false());
    
    // Check bit 3 of 8
    let result = eval.eval_str("(bit-set? 8 3)").unwrap();
    assert!(lisp.get(result).unwrap().is_true());
}

#[test]
fn test_bit_extract() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    register_embedded_natives(&mut eval).unwrap();
    
    // Extract 4 bits from position 4 of 43981 (0xABCD)
    // 0xABCD = 0b1010101111001101
    // Bits 4-7 = 0b1100 = 12
    let result = eval.eval_str("(bit-extract 43981 4 4)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(12));
}

#[test]
fn test_bit_insert() {
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    register_embedded_natives(&mut eval).unwrap();
    
    // Insert 15 (0xF) into bits 4-7 of 0
    // Result should be 0xF0 = 240
    let result = eval.eval_str("(bit-insert 0 15 4 4)").unwrap();
    assert_eq!(lisp.get(result).unwrap().as_number(), Some(240));
}
