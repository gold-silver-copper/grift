use grift::Lisp;
use rustyline::DefaultEditor;

const PROMPT: &str = "Λ> ";
const ARENA_SIZE: usize = 100_000;

fn main() {
    let lisp: Lisp<ARENA_SIZE> = Lisp::new();
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
