//! REPL and file-runner binary for Grift.
//!
//! This target is only built when the `repl` feature is enabled. It provides a
//! small command-line interface that either:
//!
//! - evaluates a source file passed as the first positional argument, or
//! - starts an interactive line-editing REPL backed by `rustyline`

use grift::Lisp;
use rustyline::DefaultEditor;

/// Prompt shown for each interactive input line.
const PROMPT: &str = "Λ> ";
/// Entry point for the feature-gated Grift CLI.
///
/// The process constructs one interpreter instance and reuses it for the
/// entire session so that top-level bindings persist across inputs.
fn main() {
    let lisp = Lisp::new();

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
            Err(
                rustyline::error::ReadlineError::Interrupted | rustyline::error::ReadlineError::Eof,
            ) => {
                break;
            }
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }
    }
}
