//! # Grift REPL Binary
//!
//! Run with: `cargo run -p grift --features std`
//! Or after installing: `grift`

fn main() {
    // Use a 50,000 cell arena (should be plenty for interactive use)
    grift::run_repl::<50000>();
}
