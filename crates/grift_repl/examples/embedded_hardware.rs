//! # Embedded Hardware Access Example
//!
//! This example demonstrates how to use the pwn_arena_embedded crate
//! to access simulated hardware registers and memory from Lisp code.
//!
//! Run with: `cargo run --example embedded_hardware`

use grift_eval::{Lisp, Evaluator};
use pwn_arena_embedded::{register_embedded_natives, reset_mock_hardware};

fn main() {
    println!("=== Embedded Hardware Access Example ===\n");
    
    // Reset mock hardware to a known state
    reset_mock_hardware();
    
    // Create a Lisp context with a 10,000 cell arena
    let lisp: Lisp<10000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();
    
    // Register all embedded native functions
    register_embedded_natives(&mut eval).unwrap();
    
    println!("Registered embedded functions:");
    println!("  Memory: peek, poke, peek32, poke32");
    println!("  GPIO: gpio-read, gpio-write, gpio-set, gpio-clear, gpio-toggle");
    println!("  Bits: bit-set?, bit-extract, bit-insert\n");
    
    // =========================================================================
    // Memory Access Examples
    // =========================================================================
    
    println!("--- Memory Access ---\n");
    
    let memory_tests = [
        ("(poke 0 42)", "Write 42 to address 0"),
        ("(peek 0)", "Read from address 0"),
        ("(poke 1 255)", "Write 255 to address 1"),
        ("(peek 1)", "Read from address 1"),
        ("(poke32 100 305419896)", "Write 0x12345678 to address 100 (32-bit)"),
        ("(peek32 100)", "Read 32-bit from address 100"),
    ];
    
    for (expr, desc) in memory_tests {
        match eval.eval_str(expr) {
            Ok(result) => {
                let value = lisp.get(result).unwrap();
                println!("  {}: {} => {:?}", desc, expr, value);
            }
            Err(e) => {
                println!("  {}: {} => ERROR: {:?}", desc, expr, e.kind);
            }
        }
    }
    
    // =========================================================================
    // GPIO Examples
    // =========================================================================
    
    println!("\n--- GPIO Control ---\n");
    
    let gpio_tests = [
        ("(gpio-write 0 0)", "Clear GPIO register 0"),
        ("(gpio-set 0 0)", "Set bit 0 of GPIO 0"),
        ("(gpio-read 0)", "Read GPIO 0"),
        ("(gpio-set 0 3)", "Set bit 3 of GPIO 0"),
        ("(gpio-read 0)", "Read GPIO 0"),
        ("(gpio-toggle 0 0)", "Toggle bit 0 of GPIO 0"),
        ("(gpio-read 0)", "Read GPIO 0"),
        ("(gpio-clear 0 3)", "Clear bit 3 of GPIO 0"),
        ("(gpio-read 0)", "Read GPIO 0"),
    ];
    
    for (expr, desc) in gpio_tests {
        match eval.eval_str(expr) {
            Ok(result) => {
                let value = lisp.get(result).unwrap();
                println!("  {}: {} => {:?}", desc, expr, value);
            }
            Err(e) => {
                println!("  {}: {} => ERROR: {:?}", desc, expr, e.kind);
            }
        }
    }
    
    // =========================================================================
    // Bit Manipulation Examples
    // =========================================================================
    
    println!("\n--- Bit Manipulation ---\n");
    
    let bit_tests = [
        ("(bit-set? 255 0)", "Is bit 0 set in 255?"),
        ("(bit-set? 255 8)", "Is bit 8 set in 255?"),
        ("(bit-extract 43981 4 4)", "Extract 4 bits from position 4 of 0xABCD"),
        ("(bit-insert 0 15 4 4)", "Insert 0xF at bits 4-7 of 0"),
    ];
    
    for (expr, desc) in bit_tests {
        match eval.eval_str(expr) {
            Ok(result) => {
                let value = lisp.get(result).unwrap();
                println!("  {}: {} => {:?}", desc, expr, value);
            }
            Err(e) => {
                println!("  {}: {} => ERROR: {:?}", desc, expr, e.kind);
            }
        }
    }
    
    // =========================================================================
    // Practical Example: LED Control Simulation
    // =========================================================================
    
    println!("\n--- Practical Example: LED Control ---\n");
    
    // Define some helper functions in Lisp
    let setup = r#"
        ; Define LED pins
        (define LED-RED 0)
        (define LED-GREEN 1)
        (define LED-BLUE 2)
        
        ; LED control functions
        (define (led-on led) (gpio-set 0 led))
        (define (led-off led) (gpio-clear 0 led))
        (define (led-toggle led) (gpio-toggle 0 led))
        (define (leds-state) (gpio-read 0))
    "#;
    
    // Execute setup
    for line in setup.lines() {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with(';') {
            let _ = eval.eval_str(line);
        }
    }
    
    println!("  Defined LED pins: RED=0, GREEN=1, BLUE=2");
    println!("  Defined functions: led-on, led-off, led-toggle, leds-state\n");
    
    // Reset GPIO before LED demo
    reset_mock_hardware();
    
    let led_tests = [
        ("(leds-state)", "Initial state"),
        ("(led-on LED-RED)", "Turn on RED LED"),
        ("(leds-state)", "State after RED on"),
        ("(led-on LED-GREEN)", "Turn on GREEN LED"),
        ("(leds-state)", "State after GREEN on"),
        ("(led-toggle LED-RED)", "Toggle RED LED"),
        ("(leds-state)", "State after toggling RED"),
        ("(led-off LED-GREEN)", "Turn off GREEN LED"),
        ("(leds-state)", "Final state"),
    ];
    
    for (expr, desc) in led_tests {
        match eval.eval_str(expr) {
            Ok(result) => {
                let value = lisp.get(result).unwrap();
                println!("  {}: {} => {:?}", desc, expr, value);
            }
            Err(e) => {
                println!("  {}: {} => ERROR: {:?}", desc, expr, e.kind);
            }
        }
    }
    
    println!("\n=== Example Complete ===");
}
