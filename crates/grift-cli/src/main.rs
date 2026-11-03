//! The grift runner. `grift SCRIPT ARGUMENT...` runs the script, which gets the arguments. With no
//! script it reads the program from stdin; when stdin is a terminal it evaluates each complete
//! input and prints the result. Data goes to stdout, diagnostics to stderr. Exit status: 0, 1 on a grift error or
//! failed output, 2 on a usage error or unreadable input, or what the program gave `exit`. A
//! closed stdout (a broken pipe) ends the run quietly.
//!
//! For scripting, the program gets a small fixed set of operations: `display`, `display-error`,
//! `read-file`, `standard-input`, `standard-output`, `standard-error`, `open-file`, `create-file`,
//! `run-process` and `load` (made by `runner.grift`), `command-line-arguments`, `script-path`,
//! `environment-variable`, `wall-clock-nanoseconds`, `elapsed-nanoseconds` and `exit`.

use grift::{Error, Grift, HostFunction, Interrupter, Value, WriteError};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

const USAGE: &str = "usage: grift [SCRIPT [ARGUMENT...]]\n  with no SCRIPT, reads the program \
                     from stdin (use `rlwrap grift` for line editing)";

/// The setup of the operations on files, called with the host functions below.
const RUNNER: &str = include_str!("runner.grift");

/// Set once a write to stdout found the pipe closed: the run then ends quietly.
static CLOSED: AtomicBool = AtomicBool::new(false);

/// A file a program opened: its path, and what is left of it until it is closed.
struct Opened {
    path: String,
    file: Option<Stream>,
}

enum Stream {
    Reading(Box<dyn BufRead>),
    Writing(Box<dyn Write>),
}

/// The runner's state: the files opened, standard input, error and output first, the status `exit`
/// gave, what stops the program when it does, and when the run started.
struct Runner {
    files: Vec<Opened>,
    exit: Option<u8>,
    stop: Interrupter,
    started: Instant,
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

/// Writes the values to `w`, strings without quotes when `raw`, then `end`, and flushes. A broken
/// pipe ends the run quietly only when `w` is stdout.
fn put(
    g: &mut G,
    values: &[Value],
    raw: bool,
    end: &str,
    w: &mut dyn Write,
    stdout: bool,
) -> Result<Value, Error> {
    let mut out = Sink { w, error: None };
    let mut written = values.iter().try_for_each(|v| match g.as_str(v) {
        Ok(Some(s)) if raw => fmt::Write::write_str(&mut out, s).map_err(|_| WriteError::Output),
        _ => g.write(v, &mut out),
    });
    written =
        written.and_then(|()| fmt::Write::write_str(&mut out, end).map_err(|_| WriteError::Output));
    if written.is_ok()
        && let Err(e) = out.w.flush()
    {
        out.error = Some(e);
        written = Err(WriteError::Output);
    }
    match (written, out.error) {
        (Ok(()), _) => Ok(Value::Inert),
        (Err(WriteError::Value(e)), _) => Err(e),
        (Err(_), Some(e)) if stdout && e.kind() == io::ErrorKind::BrokenPipe => {
            CLOSED.store(true, Ordering::Relaxed);
            Err(g.error("stdout-closed", &[]))
        }
        (Err(_), _) => Err(g.error("output-error", values)),
    }
}

/// `(display v ...)`: strings without quotes, everything else as written.
fn display(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    put(g, args, true, "", &mut io::stdout().lock(), true)
}

/// `(display-error v ...)`: as `display`, to stderr.
fn display_error(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    put(g, args, true, "", &mut io::stderr().lock(), false)
}

/// `(read-file path)`: the whole text of the file.
fn read_file(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let path = one_string(g, args)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => g.string(&text),
        Err(e) => Err(failed(g, "file-error", &path, &e)),
    }
}

/// `(resolve-host base path)`: `path`, relative to the directory of the file `base` if it is
/// relative.
fn resolve_host(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let (Some(base), Some(path)) = (args.first(), args.get(1)) else {
        return Err(g.error("arity-error", args));
    };
    let (Some(base), Some(path)) = (g.as_str(base)?, g.as_str(path)?) else {
        return Err(g.error("type-error", args));
    };
    let full = Path::new(base).parent().unwrap_or(Path::new("")).join(path);
    match full.to_str().map(String::from) {
        Some(full) => g.string(&full),
        None => Err(g.error("encoding-error", args)),
    }
}

/// `(wall-clock-nanoseconds)`: the nanoseconds since 1970-01-01 00:00 UTC by the system's clock,
/// which may be set back.
fn wall_clock_nanoseconds(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    nanoseconds(
        g,
        args,
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok(),
    )
}

/// `(elapsed-nanoseconds)`: the nanoseconds since the run started, by a clock that never goes back.
fn elapsed_nanoseconds(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    nanoseconds(g, args, Some(r.started.elapsed()))
}

/// A duration as an integer of nanoseconds; `range-error` before 1970 or after 2262.
fn nanoseconds(g: &mut G, args: &[Value], d: Option<Duration>) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    match d.and_then(|d| i64::try_from(d.as_nanos()).ok()) {
        Some(n) => Ok(Value::Integer(n)),
        None => Err(g.error("range-error", &[])),
    }
}

/// The one string argument, or a `type-error` or an `arity-error`.
fn one_string(g: &mut G, args: &[Value]) -> Result<String, Error> {
    match args {
        [v] => match g.as_string(v)? {
            Some(s) => Ok(s),
            None => Err(g.error("type-error", args)),
        },
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
    opened(g, r, args, |path| {
        Ok(Stream::Writing(Box::new(File::create(path)?)))
    })
}

/// The open file whose number is `id`, and its path; `closed` once it is closed.
fn file<'r>(g: &mut G, r: &'r mut Runner, id: &Value) -> Result<(&'r str, &'r mut Stream), Error> {
    let opened = match id {
        Value::Integer(n) => usize::try_from(*n).ok().and_then(|i| r.files.get_mut(i)),
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
        None => Err(g.error("type-error", std::slice::from_ref(id))),
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
    let (path, Stream::Reading(input)) = file(g, r, id)? else {
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

/// `(read-bytes count)` on a file: the next `count` bytes, fewer at the end, as a list of integers
/// 0-255; `()` at the end.
fn read_bytes_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, count] = args else {
        return Err(g.error("arity-error", args));
    };
    let &Value::Integer(n) = count else {
        return Err(g.error("type-error", std::slice::from_ref(count)));
    };
    let n = u64::try_from(n).map_err(|_| g.error("range-error", std::slice::from_ref(count)))?;
    let (path, Stream::Reading(input)) = file(g, r, id)? else {
        return Err(g.error("type-error", args));
    };
    let mut bytes = Vec::new();
    if let Err(e) = input.as_mut().take(n).read_to_end(&mut bytes) {
        return Err(failed(g, "file-error", path, &e));
    }
    bytes.iter().rev().try_fold(Value::EmptyList, |list, &b| {
        g.pair(Value::Integer(i64::from(b)), list)
    })
}

/// `(write text)` on a file: all of the string's bytes.
fn write_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, text] = args else {
        return Err(g.error("arity-error", args));
    };
    let (path, Stream::Writing(out)) = file(g, r, id)? else {
        return Err(g.error("type-error", args));
    };
    let Some(text) = g.as_str(text)? else {
        return Err(g.error("type-error", std::slice::from_ref(text)));
    };
    let wrote = out.write_all(text.as_bytes()).and_then(|()| out.flush());
    written(g, id, path, wrote)
}

/// `(write-bytes bytes)` on a file: the list of integers 0-255, as bytes.
fn write_bytes_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, list] = args else {
        return Err(g.error("arity-error", args));
    };
    let mut bytes = Vec::new();
    for b in g.list_items(list)? {
        match b {
            Value::Integer(n) => match u8::try_from(n) {
                Ok(b) => bytes.push(b),
                Err(_) => return Err(g.error("range-error", &[b])),
            },
            _ => return Err(g.error("type-error", &[b])),
        }
    }
    let (path, Stream::Writing(out)) = file(g, r, id)? else {
        return Err(g.error("type-error", args));
    };
    let wrote = out.write_all(&bytes).and_then(|()| out.flush());
    written(g, id, path, wrote)
}

/// After a write to file `id`: `#inert`, or the error; a closed stdout ends the run quietly, as
/// for `display`.
fn written(g: &mut G, id: &Value, path: &str, wrote: io::Result<()>) -> Result<Value, Error> {
    match wrote {
        Ok(()) => Ok(Value::Inert),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe && *id == Value::Integer(2) => {
            CLOSED.store(true, Ordering::Relaxed);
            Err(g.error("stdout-closed", &[]))
        }
        Err(e) => Err(failed(g, "file-error", path, &e)),
    }
}

/// `(close)` on a file: closes it; using it after this is the error `closed`.
fn close_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id] = args else {
        return Err(g.error("arity-error", args));
    };
    file(g, r, id)?;
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
            r.stop.stop();
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
    for v in args {
        match g.as_string(v)? {
            Some(s) => words.push(s),
            None => return Err(g.error("type-error", std::slice::from_ref(v))),
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

/// Prints an error to stderr as its kind followed by its values and where it happened, the
/// innermost combination on the error's line and each one around it on a line of its own, unless
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
    let _ = g.describe(e, &mut err);
    let _ = fmt::Write::write_str(&mut err, "\n");
    ExitCode::FAILURE
}

/// After an evaluation: the status `exit` gave, or success once stdout closed.
fn ended(r: &Runner) -> Option<ExitCode> {
    match r.exit {
        Some(status) => Some(ExitCode::from(status)),
        None if CLOSED.load(Ordering::Relaxed) => Some(ExitCode::SUCCESS),
        None => None,
    }
}

/// Whether `e` is a parse error for a list or string left open: the input isn't finished yet.
fn unfinished(g: &mut G, e: &Error) -> bool {
    let reason = match e.values.first() {
        Some(Value::Symbol(s)) => g.symbol_name(s).unwrap_or_default(),
        _ => "",
    };
    g.is_kind(e, "parse-error").unwrap_or(false)
        && ["unclosed-list", "unclosed-string"].contains(&reason)
}

fn interactive(g: &mut G, r: &mut Runner, top: &Value) -> ExitCode {
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
        // Read first: a `parse-error` from running the input (`read-form`'s) doesn't mean there is
        // more of it to come.
        pending = g.read(&src).err().filter(|e| unfinished(g, e));
        if pending.is_some() {
            eprint!("  ");
            continue;
        }
        let done = match g.evaluate_in(r, top, "stdin", &src) {
            Ok(v) => put(g, &[v], false, "\n", &mut io::stdout().lock(), true),
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
const PUBLIC: [(&str, HostFunction<Runner>); 7] = [
    ("display", display),
    ("display-error", display_error),
    ("read-file", read_file),
    ("environment-variable", environment_variable),
    ("wall-clock-nanoseconds", wall_clock_nanoseconds),
    ("elapsed-nanoseconds", elapsed_nanoseconds),
    ("exit", exit),
];

/// The host functions only `runner.grift`'s closures get.
const PRIVATE: [(&str, HostFunction<Runner>); 11] = [
    ("open-host", open_host),
    ("create-host", create_host),
    ("read-line-host", read_line_host),
    ("read-all-host", read_all_host),
    ("read-bytes-host", read_bytes_host),
    ("write-host", write_host),
    ("write-bytes-host", write_bytes_host),
    ("close-host", close_host),
    ("run-host", run_host),
    ("load-host", load_host),
    ("resolve-host", resolve_host),
];

/// `(load-host path caller)`: evaluates the forms of the file in the caller's context, each
/// combination with its line in the file.
fn load_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [path, caller] = args else {
        return Err(g.error("arity-error", args));
    };
    let path = one_string(g, std::slice::from_ref(path))?;
    let text = std::fs::read_to_string(&path).map_err(|e| failed(g, "file-error", &path, &e))?;
    g.evaluate_in(r, caller, &path, &text)?;
    Ok(Value::Inert)
}

/// Binds the runner's operations in the top-level context, and `script-path` if there is a script.
fn set_up(
    g: &mut G,
    r: &mut Runner,
    arguments: &[String],
    script: Option<&str>,
) -> Result<(), Error> {
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
    if let Some(script) = script {
        let path = g.string(script)?;
        g.define("script-path", path)?;
    }
    // The setup is made in a private context, so the operations it makes look names up there,
    // never where the program could redefine them. It counts as library code from then on, so
    // an error's trace has only the program's combinations.
    let private = g.private_context()?;
    let setup = g.evaluate_in(r, &private, "runner.grift", RUNNER)?;
    let script = g.string(script.unwrap_or_default())?;
    let mut hosts = vec![g.top_level(), script];
    for (name, f) in PRIVATE {
        hosts.push(g.host_function(name, f)?);
    }
    g.call(r, &setup, &hosts)?;
    g.seal_library();
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
    let mut r = Runner {
        files: vec![
            Opened {
                path: String::from("standard-input"),
                file: Some(Stream::Reading(Box::new(BufReader::new(io::stdin())))),
            },
            Opened {
                path: String::from("standard-error"),
                file: Some(Stream::Writing(Box::new(io::stderr()))),
            },
            Opened {
                path: String::from("standard-output"),
                file: Some(Stream::Writing(Box::new(io::stdout()))),
            },
        ],
        exit: None,
        stop: g.interrupter(),
        started: Instant::now(),
    };
    let script_path = script.as_ref().map(|s| s.to_string_lossy().into_owned());
    if let Err(e) = set_up(&mut g, &mut r, &arguments, script_path.as_deref()) {
        return report(&g, "grift", &e);
    }
    let top = g.top_level();
    let (name, text) = match script {
        None if io::stdin().is_terminal() => return interactive(&mut g, &mut r, &top),
        None => {
            // The program is standard input, so it can't read it as data too.
            if let Some(input) = r.files.first_mut() {
                input.file = None;
            }
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
    let text = match text {
        Ok(text) => text,
        Err(e) => {
            eprintln!("grift: {name}: {e}");
            return ExitCode::from(2);
        }
    };
    let done = g.evaluate_in(&mut r, &top, &name, &text);
    match (ended(&r), done) {
        (Some(status), _) => status,
        (None, Ok(_)) => ExitCode::SUCCESS,
        (None, Err(e)) => report(&g, &name, &e),
    }
}
