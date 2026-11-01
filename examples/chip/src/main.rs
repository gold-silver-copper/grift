//! grift on a Cortex-M board: QEMU's mps2-an385 (a Cortex-M3, which runs this Cortex-M0 build).
//! It installs an allocator, embeds grift, registers one host function and prints over
//! semihosting. Run it with `scripts/qemu.sh`.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
// cortex-m provides the single-core critical section the allocator locks with.
use cortex_m as _;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use embedded_alloc::LlffHeap as Heap;
use grift::{Error, Grift, Value};
use panic_halt as _;

#[global_allocator]
static HEAP: Heap = Heap::empty();

/// Each step is evaluated on its own and its result printed.
const PROGRAM: [&str; 5] = [
    "($define! area ($lambda (w h) (* w h)))",
    "(area 6 7)",
    "($define! area ($lambda (w h) (board-scale (* w h))))",
    "(area 6 7)",
    "(bindings (get-current-environment))",
];

/// The host function: what the board would do with a measurement, here scaling it by 1000.
fn board_scale(g: &mut Grift, _: &mut (), args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Int(n)] => n
            .checked_mul(1000)
            .map(Value::Int)
            .ok_or_else(|| g.error("overflow", args)),
        _ => Err(g.error("type-error", args)),
    }
}

fn run() -> Result<(), Error> {
    let mut g = Grift::new()?;
    g.define("board-scale", board_scale)?;
    for src in PROGRAM {
        let mut line = String::new();
        let shown = match g.eval(&mut (), src) {
            Ok(v) => g.write(v, &mut line),
            Err(e) => {
                line.push_str("error ");
                g.write(Value::Sym(e.code), &mut line)
            }
        };
        if shown.is_ok() {
            hprintln!("{} => {}", src, line);
        }
    }
    Ok(())
}

#[entry]
fn main() -> ! {
    // 2 MiB of the board's 4 MiB of data RAM; the rest holds the stack.
    unsafe {
        embedded_alloc::init!(HEAP, 2 * 1024 * 1024);
    }
    let status = match run() {
        Ok(()) => debug::EXIT_SUCCESS,
        Err(_) => {
            hprintln!("grift failed to start");
            debug::EXIT_FAILURE
        }
    };
    debug::exit(status);
    loop {}
}
