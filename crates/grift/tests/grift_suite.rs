//! Runs the grift-level test files in `tests/grift/`.
//!
//! Each file is evaluated in a fresh copy of a booted interpreter after the
//! assertion helpers in `_harness.grift`; any uncaught error fails the test.

use std::sync::OnceLock;

use grift::Grift;

fn booted() -> Grift {
    static BASE: OnceLock<Grift> = OnceLock::new();
    BASE.get_or_init(|| {
        let mut g = Grift::new();
        g.eval(include_str!("grift/_harness.grift"))
            .expect("harness loads");
        g
    })
    .clone()
}

fn run_file(name: &str, src: &str) {
    let mut g = booted();
    if let Err(e) = g.eval(src) {
        panic!("{name}: {e}");
    }
}

macro_rules! grift_tests {
    ($($test:ident => $file:literal,)*) => {
        $(
            #[test]
            fn $test() {
                run_file($file, include_str!(concat!("grift/", $file)));
            }
        )*
    };
}

grift_tests! {
    core => "core.grift",
    control => "control.grift",
    environments => "environments.grift",
    lists => "lists.grift",
    integers => "integers.grift",
    discipline => "discipline.grift",
    tail_calls => "tail.grift",
    delimited_control => "control-delimited.grift",
    errors => "errors.grift",
    effects => "effects.grift",
    contracts => "contracts.grift",
    records => "records.grift",
    strings => "strings.grift",
    rationals => "rationals.grift",
    gc => "gc.grift",
    ported => "ported.grift",
}
