// Scratch probe: evaluate each argument and print the result with timing.
fn main() {
    let t = std::time::Instant::now();
    let mut g = grift::Grift::new();
    eprintln!("boot: {:?}", t.elapsed());
    for src in std::env::args().skip(1) {
        let t = std::time::Instant::now();
        let s0 = g.steps();
        let a0 = g.heap_stats();
        let r = g.eval(&src);
        eprintln!("  steps {}", g.steps() - s0);
        let _ = a0;
        match r {
            Ok(v) => println!("{}  [{:?}]", g.write(v), t.elapsed()),
            Err(e) => println!("ERR {e}  [{:?}]", t.elapsed()),
        }
    }
    eprintln!("{:?}", g.heap_stats());
}
