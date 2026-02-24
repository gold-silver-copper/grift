use grift::Lisp;
use grift::io::PortId;
use rustyline::DefaultEditor;

const PROMPT: &str = "Λ> ";
const ARENA_SIZE: usize = 100_000;

/// Write function for the Lisp interpreter that routes output to stdout/stderr.
fn std_write(port: PortId, s: &str) {
    use std::io::Write;
    match port {
        PortId::STDOUT => { let _ = std::io::stdout().write_all(s.as_bytes()); }
        PortId::STDERR => { let _ = std::io::stderr().write_all(s.as_bytes()); }
        _ => {}
    }
}

fn main() {
    let lisp: Lisp<ARENA_SIZE> = Lisp::new();

    // Configure I/O: route display/newline output to stdout
    lisp.set_io(std_write);

    // Load prelude if bundled
    let prelude = include_str!("../prelude.grift");
    if let Err(e) = lisp.eval_to_index(prelude) {
        eprintln!("error loading prelude: {e:?}");
        std::process::exit(1);
    }

    // If a file argument is given, evaluate it and exit
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let filename = &args[1];
        let contents = std::fs::read_to_string(filename).unwrap_or_else(|e| {
            eprintln!("error reading {filename}: {e}");
            std::process::exit(1);
        });
        match lisp.eval_to_index(&contents) {
            Ok(idx) => {
                let mut buf = String::new();
                let _ = lisp.write_value(idx, &mut buf);
                if !buf.is_empty() && buf != "#inert" {
                    println!("{buf}");
                }
            }
            Err(e) => {
                eprintln!("error: {e:?}");
                std::process::exit(1);
            }
        }
        return;
    }

    let mut rl = DefaultEditor::new().expect("failed to initialize editor");

    loop {
        match rl.readline(PROMPT) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);
                match lisp.eval_to_index(line) {
                    Ok(idx) => {
                        let mut buf = String::new();
                        let _ = lisp.write_value(idx, &mut buf);
                        println!("{buf}");
                    }
                    Err(e) => eprintln!("error: {e:?}"),
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted | rustyline::error::ReadlineError::Eof) => {
                break;
            }
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }
    }
}
