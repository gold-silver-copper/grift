#![forbid(unsafe_code)]

//! # Grift REPL Binary
//!
//! Run with: `cargo run -p grift_repl`

use grift_repl::run_repl;

fn main() {
    // Use a 50,000 cell arena (should be plenty for interactive use)
    run_repl::<50000>();
}
