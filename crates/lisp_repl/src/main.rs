#![forbid(unsafe_code)]

//! # Lisp REPL Binary
//!
//! Run with: `cargo run -p lisp_repl`

use lisp_repl::run_repl;

fn main() {
    // Use a 50,000 cell arena (should be plenty for interactive use)
    run_repl::<50000>();
}
