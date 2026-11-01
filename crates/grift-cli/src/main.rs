//! The grift runner. `grift FILE...` runs the files in order in one top-level environment. With
//! no files it reads stdin; when stdin is a terminal it evaluates each complete input and
//! prints the result. Data goes to stdout, diagnostics to stderr. Exit status: 0, 1 on a grift
//! error or failed output, 2 on a usage error or unreadable input. A closed stdout (a broken
//! pipe) ends the run quietly.

use grift::{Error, Grift, Value};
use std::ffi::OsString;
use std::fmt;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

const USAGE: &str =
    "usage: grift [FILE...]\n  with no FILE, reads stdin (use `rlwrap grift` for line editing)";

/// Set once a write to stdout found the pipe closed: the run then ends quietly.
static CLOSED: AtomicBool = AtomicBool::new(false);

/// A `fmt::Write` straight into an `io::Write`, keeping the I/O error, so printing a value
/// needs no buffer that could grow without bound.
struct Sink<W: Write> {
    w: W,
    error: Option<io::Error>,
}

impl<W: Write> fmt::Write for Sink<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.w.write_all(s.as_bytes()).map_err(|e| {
            self.error = Some(e);
            fmt::Error
        })
    }
}

fn error(g: &mut Grift, code: &str, values: &[Value]) -> Result<Value, Error> {
    Err(Error {
        code: g.symbol(code)?,
        values: values.to_vec(),
    })
}

/// Writes the values to stdout, strings without quotes when `raw`, then `end`, and flushes.
fn put(g: &mut Grift, values: &[Value], raw: bool, end: &str) -> Result<Value, Error> {
    let mut out = Sink {
        w: io::stdout().lock(),
        error: None,
    };
    let mut written = values.iter().try_for_each(|&v| match g.string(v) {
        Some(s) if raw => fmt::Write::write_str(&mut out, s),
        _ => g.write(v, &mut out),
    });
    written = written.and_then(|()| fmt::Write::write_str(&mut out, end));
    if written.is_ok()
        && let Err(e) = out.w.flush()
    {
        out.error = Some(e);
        written = Err(fmt::Error);
    }
    match (written, out.error) {
        (Ok(()), _) => Ok(Value::Inert),
        (Err(_), Some(e)) if e.kind() == io::ErrorKind::BrokenPipe => {
            CLOSED.store(true, Ordering::Relaxed);
            error(g, "stdout-closed", &[])
        }
        (Err(_), Some(_)) => error(g, "output-error", values),
        // The printer stops only when its own stack can't grow.
        (Err(_), None) => error(g, "out-of-memory", &[]),
    }
}

/// `(display v ...)`: strings without quotes, everything else as written.
fn display(g: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    put(g, args, true, "")
}

/// `(newline)`.
fn newline(g: &mut Grift, args: &[Value]) -> Result<Value, Error> {
    match args {
        [] => put(g, args, true, "\n"),
        _ => error(g, "arity-error", args),
    }
}

/// Prints an error to stderr as its code followed by its values, unless stdout closed.
fn report(g: &Grift, source: &str, e: &Error) -> ExitCode {
    if CLOSED.load(Ordering::Relaxed) {
        return ExitCode::SUCCESS;
    }
    let mut err = Sink {
        w: io::stderr().lock(),
        error: None,
    };
    let _ = fmt::Write::write_fmt(&mut err, format_args!("{source}: "));
    let _ = g.write(Value::Sym(e.code), &mut err);
    for &v in &e.values {
        let _ = fmt::Write::write_str(&mut err, " ");
        let _ = g.write(v, &mut err);
    }
    let _ = fmt::Write::write_str(&mut err, "\n");
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
    let mut pending = None;
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
        pending = None;
        match g.eval(&src) {
            Err(e) if unfinished(g, &src, &e) => {
                pending = Some(e);
                eprint!("  ");
                continue;
            }
            Ok(v) => match put(g, &[v], false, "\n") {
                Ok(_) => status = ExitCode::SUCCESS,
                Err(e) => status = report(g, "stdin", &e),
            },
            Err(e) => status = report(g, "stdin", &e),
        }
        if CLOSED.load(Ordering::Relaxed) {
            return ExitCode::SUCCESS;
        }
        src.clear();
        eprint!("> ");
    }
    // End of input in the middle of a form: that form is an error.
    match pending {
        Some(e) => report(g, "stdin", &e),
        None => status,
    }
}

fn main() -> ExitCode {
    let files: Vec<OsString> = std::env::args_os().skip(1).collect();
    if files.iter().any(|f| f.as_encoded_bytes().starts_with(b"-")) {
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
        return report(&g, "grift", &e);
    }
    if files.is_empty() && io::stdin().is_terminal() {
        return interactive(&mut g);
    }
    let sources = match files.is_empty() {
        true => vec![("stdin".into(), read_stdin())],
        false => files
            .into_iter()
            .map(|f| {
                (
                    f.to_string_lossy().into_owned(),
                    std::fs::read_to_string(&f),
                )
            })
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
            return report(&g, &name, &e);
        }
    }
    ExitCode::SUCCESS
}

fn read_stdin() -> io::Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}
