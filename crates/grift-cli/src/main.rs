//! The grift runner. `grift FILE...` runs the files in order in one top-level environment. With
//! no files it reads stdin; when stdin is a terminal it evaluates each complete input and
//! prints the result. Data goes to stdout, diagnostics to stderr. Exit status: 0, 1 on a grift
//! error, 2 on a usage error or unreadable input. A closed stdout ends the run quietly.

use grift::{Error, Grift, Value};
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process::ExitCode;

const USAGE: &str =
    "usage: grift [FILE...]\n  with no FILE, reads stdin (use `rlwrap grift` for line editing)";

/// The error code a host function returns when stdout is closed.
const CLOSED: &str = "stdout-closed";

fn out(g: &mut Grift, text: &str) -> Result<Value, Error> {
    let mut stdout = io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => Ok(Value::Inert),
        Err(_) => Err(Error {
            code: g.symbol(CLOSED)?,
            values: Vec::new(),
        }),
    }
}

/// `(display v ...)`: strings without quotes, everything else as written.
fn display(g: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    let mut text = String::new();
    for &v in args {
        match g.string(v) {
            Some(s) => text.push_str(s),
            None => {
                let _ = g.write(v, &mut text);
            }
        }
    }
    out(g, &text)
}

/// `(newline)`.
fn newline(g: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    match args {
        [] => out(g, "\n"),
        _ => Err(Error {
            code: g.symbol("arity-error")?,
            values: args.to_vec(),
        }),
    }
}

fn show(g: &Grift, v: Value) -> String {
    let mut s = String::new();
    let _ = g.write(v, &mut s);
    s
}

/// Prints an error to stderr as its code followed by its values, unless stdout closed.
fn report(g: &mut Grift, source: &str, e: &Error) -> ExitCode {
    if g.symbol(CLOSED).is_ok_and(|c| c == e.code) {
        return ExitCode::SUCCESS;
    }
    let mut line = format!("{source}: {}", show(g, Value::Sym(e.code)));
    for &v in &e.values {
        line.push(' ');
        line.push_str(&show(g, v));
    }
    eprintln!("{line}");
    ExitCode::FAILURE
}

/// Whether `e` is a parse error at the very end of `src`: the input isn't finished yet.
fn unfinished(g: &mut Grift, src: &str, e: &Error) -> bool {
    let line = src.matches('\n').count() + 1;
    let col = src.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    let end = [Value::Int(line as i64), Value::Int(col as i64)];
    g.symbol("parse-error").is_ok_and(|c| c == e.code) && e.values == end
}

fn interactive(g: &mut Grift) -> ExitCode {
    let mut status = ExitCode::SUCCESS;
    let mut src = String::new();
    eprint!("> ");
    for line in io::stdin().lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("grift: stdin: {e}");
                return ExitCode::from(2);
            }
        };
        src.push_str(&line);
        src.push('\n');
        if src.trim().is_empty() {
            src.clear();
            eprint!("> ");
            continue;
        }
        match g.eval(&src) {
            Err(e) if unfinished(g, &src, &e) => {
                eprint!("  ");
                continue;
            }
            Ok(v) => {
                let text = show(g, v) + "\n";
                if out(g, &text).is_err() {
                    return ExitCode::SUCCESS;
                }
                status = ExitCode::SUCCESS;
            }
            Err(e) => status = report(g, "stdin", &e),
        }
        src.clear();
        eprint!("> ");
    }
    status
}

fn main() -> ExitCode {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.iter().any(|f| f.starts_with('-')) {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let mut g = match Grift::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!(
                "grift: the library failed to load ({} values)",
                e.values.len()
            );
            return ExitCode::FAILURE;
        }
    };
    let host = g
        .define("display", display)
        .and_then(|()| g.define("newline", newline));
    if let Err(e) = host {
        return report(&mut g, "grift", &e);
    }
    if files.is_empty() && io::stdin().is_terminal() {
        return interactive(&mut g);
    }
    let sources = match files.is_empty() {
        true => vec![("stdin".to_string(), read_stdin())],
        false => files
            .into_iter()
            .map(|f| (f.clone(), std::fs::read_to_string(&f)))
            .collect(),
    };
    for (name, text) in sources {
        let text = match text {
            Ok(text) => text,
            Err(e) => {
                eprintln!("grift: {name}: {e}");
                return ExitCode::from(2);
            }
        };
        if let Err(e) = g.eval(&text) {
            return report(&mut g, &name, &e);
        }
    }
    ExitCode::SUCCESS
}

fn read_stdin() -> io::Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}
