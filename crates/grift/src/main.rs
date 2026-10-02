//! REPL and file runner for grift (built with the `repl` feature).
//!
//! `grift FILE...` evaluates the files in order and prints the value of the
//! last one unless it is `#inert`. With no arguments it starts an
//! interactive REPL; input is read until its parentheses balance.
//!
//! Two natives give programs a way to produce output: `(print v ...)` and
//! `(newline)`. They are the only side effects in the language.

use grift::{Ctx, Error, Grift, Value};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

const PROMPT: &str = "Λ> ";
const CONTINUE: &str = ".. ";

/// `(print v ...)`: display each value, separated by spaces, then a newline.
fn print(ctx: &mut Ctx, args: &[Value]) -> Result<Value, Error> {
    let parts: Vec<String> = args.iter().map(|&v| ctx.display(v)).collect();
    println!("{}", parts.join(" "));
    Ok(Value::Inert)
}

/// `(newline)`: print a newline.
fn newline(_: &mut Ctx, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(Error::new("arity-error", "newline takes no arguments"));
    }
    println!();
    Ok(Value::Inert)
}

fn interpreter() -> Grift {
    let mut g = Grift::new();
    g.register_native("print", print).expect("valid name");
    g.register_native("newline", newline).expect("valid name");
    g
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let mut g = interpreter();
    if files.is_empty() {
        repl(&mut g);
        return;
    }
    let mut last = Value::Inert;
    for file in &files {
        let src = std::fs::read_to_string(file).unwrap_or_else(|e| {
            eprintln!("error reading {file}: {e}");
            std::process::exit(1);
        });
        match g.eval(&src) {
            Ok(v) => last = v,
            Err(e) => {
                eprintln!("{file}: {e}");
                std::process::exit(1);
            }
        }
    }
    if last != Value::Inert {
        println!("{}", g.write(last));
    }
}

/// Net count of open parentheses outside strings and comments; positive
/// means the input is incomplete.
fn open_parens(src: &str) -> i64 {
    let mut depth = 0;
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            ';' => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '#' if chars.peek() == Some(&'\\') => {
                chars.next();
                chars.next();
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn repl(g: &mut Grift) {
    let mut rl = DefaultEditor::new().expect("failed to initialize line editor");
    let mut buffer = String::new();
    loop {
        let prompt = if buffer.is_empty() { PROMPT } else { CONTINUE };
        match rl.readline(prompt) {
            Ok(line) => {
                buffer.push_str(&line);
                buffer.push('\n');
                if open_parens(&buffer) > 0 {
                    continue;
                }
                let input = std::mem::take(&mut buffer);
                if input.trim().is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input.trim());
                match g.eval(&input) {
                    Ok(Value::Inert) => {}
                    Ok(v) => println!("{}", g.write(v)),
                    Err(e) => eprintln!("error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) => buffer.clear(),
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }
    }
}
