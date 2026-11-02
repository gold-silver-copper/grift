//! The grift runner. `grift SCRIPT ARGUMENT...` runs the script, which gets the arguments; a
//! first line starting with `#!` is skipped, so a script can run itself. With no script it reads
//! the program from stdin; when stdin is a terminal it evaluates each complete input and prints
//! the result. Data goes to stdout, diagnostics to stderr. Exit status: 0, 1 on a grift error or
//! failed output, 2 on a usage error or unreadable input, or what the program gave `exit`. A
//! closed stdout (a broken pipe) ends the run quietly.
//!
//! For scripting, the program gets a small fixed set of operations: `display` and `newline`,
//! `standard-input`, `open-file`, `create-file` and `run-process` (contexts of operations, made
//! by `runner.grift`), `command-line-arguments`, `environment-variable` and `exit`.

use grift::{Error, Grift, HostFunction, Value};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};

const USAGE: &str = "usage: grift [SCRIPT [ARGUMENT...]]\n  with no SCRIPT, reads the program \
                     from stdin (use `rlwrap grift` for line editing)";

/// The setup of the operations on files, called with the host functions below.
const RUNNER: &str = include_str!("runner.grift");

/// Set once a write to stdout found the pipe closed: the run then ends quietly.
static CLOSED: AtomicBool = AtomicBool::new(false);

/// grift's interrupt flag, set by `exit` to stop the program.
static STOP: AtomicBool = AtomicBool::new(false);

/// A file a program opened: its path, and what is left of it until it is closed.
struct Opened {
    path: String,
    file: Option<Stream>,
}

enum Stream {
    Reading(Box<dyn BufRead>),
    Writing(File),
}

/// The runner's state: the files opened, standard input first, and the status `exit` gave.
struct Runner {
    files: Vec<Opened>,
    exit: Option<u8>,
}

type G = Grift<Runner>;

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

/// Writes the values to stdout, strings without quotes when `raw`, then `end`, and flushes.
fn put(g: &mut G, values: &[Value], raw: bool, end: &str) -> Result<Value, Error> {
    let mut out = Sink {
        w: io::stdout().lock(),
        error: None,
    };
    let mut written = values.iter().try_for_each(|&v| match g.as_str(v) {
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
            Err(g.error("stdout-closed", &[]))
        }
        (Err(_), Some(_)) => Err(g.error("output-error", values)),
        // The printer stops only when its own stack can't grow.
        (Err(_), None) => Err(g.error("out-of-memory", &[])),
    }
}

/// `(display v ...)`: strings without quotes, everything else as written.
fn display(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    put(g, args, true, "")
}

/// `(newline)`.
fn newline(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    match args {
        [] => put(g, args, true, "\n"),
        _ => Err(g.error("arity-error", args)),
    }
}

/// The one string argument, or a `type-error` or an `arity-error`.
fn one_string(g: &mut G, args: &[Value]) -> Result<String, Error> {
    match args {
        [v] => g
            .as_str(*v)
            .map(String::from)
            .ok_or_else(|| g.error("type-error", args)),
        _ => Err(g.error("arity-error", args)),
    }
}

/// An I/O error about `what` (a path or a program): `encoding-error` for bytes that aren't
/// UTF-8, else `kind` with the system's message.
fn failed(g: &mut G, kind: &str, what: &str, e: &io::Error) -> Error {
    let what = match g.string(what) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if e.kind() == io::ErrorKind::InvalidData {
        return g.error("encoding-error", &[what]);
    }
    match g.string(&e.to_string()) {
        Ok(message) => g.error(kind, &[what, message]),
        Err(e) => e,
    }
}

/// Opens a file as `open` does, and gives its number.
fn opened(
    g: &mut G,
    r: &mut Runner,
    args: &[Value],
    open: fn(&str) -> io::Result<Stream>,
) -> Result<Value, Error> {
    let path = one_string(g, args)?;
    match open(&path) {
        Ok(file) => {
            let id = i64::try_from(r.files.len()).map_err(|_| g.error("out-of-memory", &[]))?;
            r.files.push(Opened {
                path,
                file: Some(file),
            });
            Ok(Value::Integer(id))
        }
        Err(e) => Err(failed(g, "file-error", &path, &e)),
    }
}

fn open_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    opened(g, r, args, |path| {
        Ok(Stream::Reading(Box::new(BufReader::new(File::open(path)?))))
    })
}

fn create_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    opened(g, r, args, |path| Ok(Stream::Writing(File::create(path)?)))
}

/// The open file whose number is `id`, and its path; `closed` once it is closed.
fn file<'r>(g: &mut G, r: &'r mut Runner, id: Value) -> Result<(&'r str, &'r mut Stream), Error> {
    let opened = match id {
        Value::Integer(n) => usize::try_from(n).ok().and_then(|i| r.files.get_mut(i)),
        _ => None,
    };
    match opened {
        Some(Opened {
            path,
            file: Some(stream),
        }) => Ok((path, stream)),
        Some(Opened { path, file: None }) => {
            let path = g.string(path)?;
            Err(g.error("closed", &[path]))
        }
        None => Err(g.error("type-error", &[id])),
    }
}

/// Reads with `read` from the file whose number is the only argument.
fn reading(
    g: &mut G,
    r: &mut Runner,
    args: &[Value],
    read: fn(&mut dyn BufRead, &mut String) -> io::Result<usize>,
) -> Result<Value, Error> {
    let [id] = args else {
        return Err(g.error("arity-error", args));
    };
    let (path, Stream::Reading(input)) = file(g, r, *id)? else {
        return Err(g.error("type-error", args));
    };
    let mut s = String::new();
    match read(input.as_mut(), &mut s) {
        Ok(_) => g.string(&s),
        Err(e) => Err(failed(g, "file-error", path, &e)),
    }
}

/// `(read-line)` on a file: the next line with its newline, `""` at the end (Rust's
/// `BufRead::read_line`).
fn read_line_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    reading(g, r, args, |input, s| input.read_line(s))
}

/// `(read-all)` on a file: the rest of it.
fn read_all_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    reading(g, r, args, |input, s| input.read_to_string(s))
}

/// `(write text)` on a file: all of the string's bytes.
fn write_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, text] = args else {
        return Err(g.error("arity-error", args));
    };
    let (path, Stream::Writing(out)) = file(g, r, *id)? else {
        return Err(g.error("type-error", args));
    };
    let Some(text) = g.as_str(*text) else {
        return Err(g.error("type-error", &[*text]));
    };
    match out.write_all(text.as_bytes()) {
        Ok(()) => Ok(Value::Inert),
        Err(e) => Err(failed(g, "file-error", path, &e)),
    }
}

/// `(close)` on a file: closes it; using it after this is the error `closed`.
fn close_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id] = args else {
        return Err(g.error("arity-error", args));
    };
    file(g, r, *id)?;
    if let Value::Integer(n) = id
        && let Some(opened) = usize::try_from(*n).ok().and_then(|i| r.files.get_mut(i))
    {
        opened.file = None;
    }
    Ok(Value::Inert)
}

/// `(environment-variable name)`: its value; `unset-variable` when it has none.
fn environment_variable(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let name = one_string(g, args)?;
    match std::env::var(&name) {
        Ok(value) => g.string(&value),
        Err(std::env::VarError::NotPresent) => Err(g.error("unset-variable", args)),
        Err(std::env::VarError::NotUnicode(_)) => Err(g.error("encoding-error", args)),
    }
}

/// `(exit status)`: ends the run with `status`, 0 to 255. The program stops at its next step,
/// as when the host interrupts it, so no `catch!` can keep it going; its cleanups run.
fn exit(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Integer(n)] => {
            let status = u8::try_from(*n).map_err(|_| g.error("range-error", args))?;
            r.exit = Some(status);
            STOP.store(true, Ordering::Relaxed);
            Ok(Value::Inert)
        }
        [_] => Err(g.error("type-error", args)),
        _ => Err(g.error("arity-error", args)),
    }
}

/// `(run-host program argument ...)`: runs it to its end, with no input, and gives its exit code
/// (`#inert` if a signal ended it) and what it wrote to stdout and stderr, as a list.
fn run_host(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let mut words = Vec::new();
    for &v in args {
        match g.as_str(v) {
            Some(s) => words.push(String::from(s)),
            None => return Err(g.error("type-error", &[v])),
        }
    }
    let Some((program, arguments)) = words.split_first() else {
        return Err(g.error("arity-error", args));
    };
    let ran = match Command::new(program).args(arguments).output() {
        Ok(ran) => ran,
        Err(e) => return Err(failed(g, "process-error", program, &e)),
    };
    let status = ran
        .status
        .code()
        .map_or(Value::Inert, |n| Value::Integer(i64::from(n)));
    let mut list = Value::EmptyList;
    for bytes in [ran.stderr, ran.stdout] {
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(g.error("encoding-error", args));
        };
        let text = g.string(&text)?;
        list = g.pair(text, list)?;
    }
    g.pair(status, list)
}

/// Prints an error to stderr as its kind followed by its values and where it happened, unless
/// stdout closed.
fn report(g: &G, source: &str, e: &Error) -> ExitCode {
    if CLOSED.load(Ordering::Relaxed) {
        return ExitCode::SUCCESS;
    }
    let mut err = Sink {
        w: io::stderr().lock(),
        error: None,
    };
    let _ = fmt::Write::write_fmt(&mut err, format_args!("{source}: "));
    let _ = g.write(Value::Symbol(e.kind), &mut err);
    for &v in &e.values {
        let _ = fmt::Write::write_str(&mut err, " ");
        let _ = g.write(v, &mut err);
    }
    // The innermost combination on the error's line, then each one around it on a line of
    // its own.
    for (i, &at) in e.trace.iter().enumerate() {
        let _ = fmt::Write::write_str(&mut err, if i == 0 { " in " } else { "\n  from " });
        let _ = g.write(at, &mut err);
    }
    let _ = fmt::Write::write_str(&mut err, "\n");
    ExitCode::FAILURE
}

/// After an evaluation: the status `exit` gave, or success once stdout closed.
fn ended(r: &Runner) -> Option<ExitCode> {
    STOP.store(false, Ordering::Relaxed);
    match r.exit {
        Some(status) => Some(ExitCode::from(status)),
        None if CLOSED.load(Ordering::Relaxed) => Some(ExitCode::SUCCESS),
        None => None,
    }
}

/// Whether `e` is a parse error at the very end of `src`: the input isn't finished yet.
fn unfinished(g: &mut G, src: &str, e: &Error) -> bool {
    let line = src.matches('\n').count() + 1;
    let col = src.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    let end = [Value::Integer(line as i64), Value::Integer(col as i64)];
    g.symbol("parse-error").is_ok_and(|k| k == e.kind) && e.values == end
}

fn interactive(g: &mut G, r: &mut Runner) -> ExitCode {
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
        let done = match g.evaluate(r, &src) {
            Err(e) if unfinished(g, &src, &e) => {
                pending = Some(e);
                eprint!("  ");
                continue;
            }
            Ok(v) => put(g, &[v], false, "\n"),
            Err(e) => Err(e),
        };
        if let Some(status) = ended(r) {
            return status;
        }
        status = match done {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => report(g, "stdin", &e),
        };
        src.clear();
        eprint!("> ");
    }
    // End of input in the middle of a form: that form is an error.
    match pending {
        Some(e) => report(g, "stdin", &e),
        None => status,
    }
}

/// The host functions the program gets by name.
const PUBLIC: [(&str, HostFunction<Runner>); 4] = [
    ("display", display),
    ("newline", newline),
    ("environment-variable", environment_variable),
    ("exit", exit),
];

/// The host functions only `runner.grift`'s closures get.
const PRIVATE: [(&str, HostFunction<Runner>); 7] = [
    ("open-host", open_host),
    ("create-host", create_host),
    ("read-line-host", read_line_host),
    ("read-all-host", read_all_host),
    ("write-host", write_host),
    ("close-host", close_host),
    ("run-host", run_host),
];

/// Binds the runner's operations in the top-level context.
fn set_up(g: &mut G, r: &mut Runner, arguments: &[String]) -> Result<(), Error> {
    for (name, f) in PUBLIC {
        let f = g.host_function(name, f)?;
        g.define(name, f)?;
    }
    let mut list = Value::EmptyList;
    for a in arguments.iter().rev() {
        let a = g.string(a)?;
        list = g.pair(a, list)?;
    }
    g.define("command-line-arguments", list)?;
    // The setup is made in a private context, so the operations it makes look names up there,
    // never where the program could redefine them. An evaluation may reclaim a value held only
    // here, so `form`, `private`, `setup` and the host functions are each made after the last
    // evaluation before their use; `top` and `evaluate` are always kept.
    let (top, evaluate) = (
        g.evaluate(r, "(current-context)")?,
        g.evaluate(r, "evaluate")?,
    );
    let forms = g.read(RUNNER)?;
    // runner.grift is one form, the setup combiner.
    let Some((form, Value::EmptyList)) = g.as_pair(forms) else {
        return Err(g.error("parse-error", &[forms]));
    };
    let private = g.context()?;
    let setup = g.call(r, evaluate, &[form, private])?;
    let mut hosts = vec![top];
    for (name, f) in PRIVATE {
        hosts.push(g.host_function(name, f)?);
    }
    g.call(r, setup, &hosts)?;
    Ok(())
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let script = args.next();
    if script
        .as_ref()
        .is_some_and(|s| s.as_encoded_bytes().starts_with(b"-"))
    {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let arguments: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
    let mut r = Runner {
        files: vec![Opened {
            path: String::from("standard-input"),
            file: Some(Stream::Reading(Box::new(BufReader::new(io::stdin())))),
        }],
        exit: None,
    };
    let mut g: G = match Grift::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!(
                "grift: the library failed to load ({} values)",
                e.values.len()
            );
            return ExitCode::FAILURE;
        }
    };
    g.set_interrupt_flag(&STOP);
    if let Err(e) = set_up(&mut g, &mut r, &arguments) {
        return report(&g, "grift", &e);
    }
    let (name, text) = match script {
        None if io::stdin().is_terminal() => return interactive(&mut g, &mut r),
        None => {
            // The program is standard input, so it can't read it as data too.
            r.files.iter_mut().for_each(|f| f.file = None);
            let mut s = String::new();
            (
                String::from("stdin"),
                io::stdin().read_to_string(&mut s).map(|_| s),
            )
        }
        Some(path) => (
            path.to_string_lossy().into_owned(),
            std::fs::read_to_string(&path),
        ),
    };
    let mut text = match text {
        Ok(text) => text,
        Err(e) => {
            eprintln!("grift: {name}: {e}");
            return ExitCode::from(2);
        }
    };
    // `#!/usr/bin/env grift` reads as an empty first line, so lines keep their numbers.
    if text.starts_with("#!") {
        let end = text.find('\n').unwrap_or(text.len());
        text.replace_range(..end, "");
    }
    let done = g.evaluate(&mut r, &text);
    match (ended(&r), done) {
        (Some(status), _) => status,
        (None, Ok(_)) => ExitCode::SUCCESS,
        (None, Err(e)) => report(&g, &name, &e),
    }
}
