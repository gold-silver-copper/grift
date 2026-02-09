//! # Embedded I/O Example
//!
//! Demonstrates how to use [`EmbeddedIoProvider`] to feed input and capture
//! output without `std` or heap allocation — the same pattern you would use
//! on a bare-metal target.
//!
//! Run with: `cargo run -p grift_arena_embedded --example embedded_io`

use grift_eval::{Lisp, Evaluator, IoProvider, PortId};
use grift_arena_embedded::EmbeddedIoProvider;

fn main() {
    println!("=== Embedded I/O Provider Example ===\n");

    // ------------------------------------------------------------------
    // 1.  Basic read / write through EmbeddedIoProvider
    // ------------------------------------------------------------------
    println!("--- 1. Basic read / write ---\n");

    // Create an I/O provider with a 512-byte buffer (no heap allocation).
    let mut io: EmbeddedIoProvider<512> = EmbeddedIoProvider::new();

    // Pre-load input that `read_char` will consume.
    io.load_input("AB");

    let a = io.read_char(PortId::STDIN).unwrap();
    let b = io.read_char(PortId::STDIN).unwrap();
    println!("  read_char => '{}', '{}'", a, b);

    // Write output.
    io.write_str(PortId::STDOUT, "Hello from embedded!").unwrap();
    println!("  output_str => {:?}", io.output_str());

    // ------------------------------------------------------------------
    // 2.  Feeding input to the Scheme evaluator
    // ------------------------------------------------------------------
    println!("\n--- 2. Evaluator with embedded I/O ---\n");

    let lisp: Lisp<20000> = Lisp::new();
    let mut eval = Evaluator::new(&lisp).unwrap();

    // Reset the I/O provider for a fresh session.
    let mut io: EmbeddedIoProvider<512> = EmbeddedIoProvider::new();

    // Pre-load a character that Scheme's (read-char) will return.
    io.load_input("X");

    // Attach the I/O provider to the evaluator.
    eval.set_io_provider(&mut io);

    // Evaluate a Scheme expression that reads a character.
    let result = eval.eval_str("(read-char)").unwrap();
    let value = lisp.get(result).unwrap();
    println!("  (read-char) => {:?}", value);

    // ------------------------------------------------------------------
    // 3.  Custom buffer size
    // ------------------------------------------------------------------
    println!("\n--- 3. Custom buffer size (64 bytes) ---\n");

    let mut tiny: EmbeddedIoProvider<64> = EmbeddedIoProvider::new();
    tiny.load_input("small");

    while let Ok(c) = tiny.read_char(PortId::STDIN) {
        tiny.write_char(PortId::STDOUT, c).unwrap();
    }
    println!("  Echo result: {:?}", tiny.output_str());

    println!("\n=== Example Complete ===");
}
