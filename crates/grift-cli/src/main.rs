//! The grift runner. `grift SCRIPT ARGUMENT...` runs the script, which gets the arguments. With no
//! script it reads the program from stdin; when stdin is a terminal `runner.grift` evaluates each
//! complete input and prints the result. Data goes to stdout, diagnostics to stderr. Exit status: 0,
//! 1 on a grift error (a script that can't be read, or failed output, is one), or what the program
//! gave `exit`. Errors shown at a terminal don't count: a session ends 0 unless the program calls
//! `exit`, input ends inside a form, or standard input or error fails or is closed, which is the
//! session's own error and ends it with 1. A write to a pipe no one reads, on any file, ends the run as
//! `exit` does, by a limit of kind `broken-pipe`, quietly and with 0.
//!
//! For scripting, the program gets a small fixed set of operations: `display`, `display-error`,
//! `read-file`, `read-forms`, `position`, `standard-input`, `standard-output`, `standard-error`,
//! `open-file`, `create-file`, `run-process` and `load` (made by `runner.grift`), `command-line-arguments`, `script-path`,
//! `environment-variable`, `wall-clock-nanoseconds`, `elapsed-nanoseconds` and `exit`.

use grift::{Error, Grift, HostFunction, Value, WriteError};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant, SystemTime};

const USAGE: &str = "usage: grift [SCRIPT [ARGUMENT...]]\n  with no SCRIPT, reads the program \
                     from stdin (use `rlwrap grift` for line editing)";

/// The setup of the operations on files, called with the host functions below.
const RUNNER: &str = include_str!("runner.grift");

/// A file a program opened: its path, and what is left of it until it is closed.
struct Opened {
    path: String,
    file: Option<Stream>,
}

enum Stream {
    Reading(Box<dyn BufRead>),
    Writing(Box<dyn Write>),
}

/// The runner's state: the files opened, standard input, error and output first, when the run
/// started, and the script.
struct Runner {
    files: Vec<Opened>,
    started: Instant,
    script: Option<PathBuf>,
}

type G = Grift<Runner>;

/// A `fmt::Write` straight into an `io::Write`, keeping what failed first, so printing a value
/// needs no buffer that could grow without bound.
struct Sink<W: Write> {
    w: W,
    error: Option<io::Error>,
}

impl<W: Write> fmt::Write for Sink<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.w.write_all(s.as_bytes()).map_err(|e| {
            self.error.get_or_insert(e);
            fmt::Error
        })
    }
}

/// Writes to stderr. One that is closed loses the text, which is all that can go wrong here.
fn say(text: fmt::Arguments) {
    let _ = io::stderr().write_fmt(text);
}

/// The numbers of the standard streams that are written to, in `Runner::files`.
const STANDARD_ERROR: i64 = 1;
const STANDARD_OUTPUT: i64 = 2;

/// Writes the values to file `id`, strings without quotes and everything else as written, and
/// flushes.
fn put(g: &mut G, r: &mut Runner, id: i64, values: &[Value]) -> Result<Value, Error> {
    let id = Value::Integer(id);
    let (path, Stream::Writing(w)) = file(g, r, &id)? else {
        return Err(g.error("type-error", values));
    };
    let mut out = Sink { w, error: None };
    let printed = values.iter().try_for_each(|v| match g.as_str(v) {
        Ok(Some(s)) => fmt::Write::write_str(&mut out, s).map_err(|_| WriteError::Output),
        _ => g.write(v, &mut out),
    });
    let wrote = match (printed, out.error) {
        (Err(WriteError::Value(e)), _) => return Err(e),
        (_, Some(e)) => Err(e),
        (_, None) => out.w.flush(),
    };
    let wrote = wrote.map_err(|e| (e, path.to_owned()));
    written(g, wrote)
}

/// `(display v ...)`: strings without quotes, everything else as written.
fn display(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    put(g, r, STANDARD_OUTPUT, args)
}

/// `(display-error v ...)`: as `display`, to stderr.
fn display_error(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    put(g, r, STANDARD_ERROR, args)
}

/// `(resolve-host base path)`: `path`, relative to the directory of the file `base` if it is
/// relative. The program holds the script's path as text, which may be a lossy copy naming another
/// file, so the script's directory comes from the real path: `encoding-error` if text can't hold it.
fn resolve_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let (Some(base), Some(path)) = (args.first(), args.get(1)) else {
        return Err(g.error("arity-error", args));
    };
    let (Some(base), Some(path)) = (g.as_str(base)?, g.as_str(path)?) else {
        return Err(g.error("type-error", args));
    };
    let script = r.script.as_deref().filter(|s| s.to_string_lossy() == base);
    let full = script
        .unwrap_or(Path::new(base))
        .parent()
        .unwrap_or(Path::new(""))
        .join(path);
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

/// A copy of a string value's contents, `None` for anything else; a program that fills memory gets
/// the limit `out-of-memory`, not an abort.
fn owned_string(g: &mut G, v: &Value) -> Result<Option<String>, Error> {
    let Some(len) = g.as_str(v)?.map(str::len) else {
        return Ok(None);
    };
    let mut copy = String::new();
    if copy.try_reserve_exact(len).is_err() {
        return Err(no_memory(g));
    }
    copy.push_str(g.as_str(v)?.unwrap_or_default());
    Ok(Some(copy))
}

/// The one string argument, or a `type-error` or an `arity-error`.
fn one_string(g: &mut G, args: &[Value]) -> Result<String, Error> {
    match args {
        [v] => match owned_string(g, v)? {
            Some(s) => Ok(s),
            None => Err(g.error("type-error", args)),
        },
        _ => Err(g.error("arity-error", args)),
    }
}

/// Memory refused: the limit a real allocation failure is, wherever it happens.
fn no_memory(g: &mut G) -> Error {
    g.error("out-of-memory", &[]).as_limit()
}

/// An I/O error about `what` (a path or a program): out of memory, `encoding-error` for bytes that
/// aren't UTF-8, else `kind` with the system's message.
fn failed(g: &mut G, kind: &str, what: &str, e: &io::Error) -> Error {
    if e.kind() == io::ErrorKind::OutOfMemory {
        return no_memory(g);
    }
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

/// A failed write to the file `path`: a pipe no one reads is the limit that ends the run quietly, as
/// [`exit_status`] says, and only a real write makes it.
fn write_failed(g: &mut G, path: &str, e: &io::Error) -> Error {
    if e.kind() != io::ErrorKind::BrokenPipe {
        return failed(g, "file-error", path, e);
    }
    match g.string(path) {
        Ok(path) => g.error("broken-pipe", &[path]).as_limit(),
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
            let id = i64::try_from(r.files.len()).map_err(|_| no_memory(g))?;
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

/// `(read-bytes count)` on a file: the next `count` bytes, fewer at the end, as a bytes value;
/// `#bytes()` at the end. It reads only as much as it gives, so a pipe that stays open answers as
/// soon as `count` bytes have come.
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
    let mut input = input.as_mut().take(n);
    let mut bytes = Vec::new();
    loop {
        let chunk = match input.fill_buf() {
            Ok([]) => break,
            Ok(chunk) => chunk,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(failed(g, "file-error", path, &e)),
        };
        if bytes.try_reserve(chunk.len()).is_err() {
            return Err(no_memory(g));
        }
        bytes.extend_from_slice(chunk);
        let used = chunk.len();
        input.consume(used);
    }
    g.bytes(&bytes)
}

/// `(write data)` on a file: all of the bytes of a string or a bytes value.
fn write_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, data] = args else {
        return Err(g.error("arity-error", args));
    };
    let (path, Stream::Writing(out)) = file(g, r, id)? else {
        return Err(g.error("type-error", args));
    };
    let bytes = match g.as_str(data)? {
        Some(text) => Some(text.as_bytes()),
        None => g.as_bytes(data)?,
    };
    let Some(bytes) = bytes else {
        return Err(g.error("type-error", std::slice::from_ref(data)));
    };
    let wrote = out.write_all(bytes).and_then(|()| out.flush());
    let wrote = wrote.map_err(|e| (e, path.to_owned()));
    written(g, wrote)
}

/// After a write: `#inert`, or the error with the file's path.
fn written(g: &mut G, wrote: Result<(), (io::Error, String)>) -> Result<Value, Error> {
    match wrote {
        Ok(()) => Ok(Value::Inert),
        Err((e, path)) => Err(write_failed(g, &path, &e)),
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

/// `(forget-host)`: closes the file opened last and frees its number. For a file a program never
/// held, so that reading again and again keeps nothing: a file a program closed keeps its entry,
/// so its number is `closed` for ever and never comes to mean another file.
fn forget_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    r.files.pop();
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

/// `(exit status)`: ends the run with `status`, 0 to 255, by a limit of kind `exit`: no `catch!`
/// keeps it, each `finally!` it leaves runs its cleanup forms, and [`exit_status`] reads it at the
/// end.
fn exit(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    match args {
        [Value::Integer(n)] => {
            u8::try_from(*n).map_err(|_| g.error("range-error", args))?;
            Err(g.error("exit", args).as_limit())
        }
        [_] => Err(g.error("type-error", args)),
        _ => Err(g.error("arity-error", args)),
    }
}

/// The status a run ended with by the runner's own limits, which a program can't begin: what `exit`
/// gave, or 0 for a pipe no one reads, so `grift script | head` is quiet.
fn exit_status(g: &G, e: &Error) -> Option<u8> {
    if !e.is_limit() {
        return None;
    }
    match (g.symbol_name(&e.kind).ok()?, e.values.as_slice()) {
        ("broken-pipe", _) => Some(0),
        ("exit", [Value::Integer(n)]) => u8::try_from(*n).ok(),
        _ => None,
    }
}

/// `(run-host program argument ...)`: runs it to its end, with no input, and gives its exit code
/// (`#inert` if a signal ended it) and what it wrote to stdout and stderr, as a list.
fn run_host(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let mut words = Vec::new();
    for v in args {
        match owned_string(g, v)? {
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

/// Prints an error to stderr as its source, then its kind followed by its values and where it happened, the
/// innermost combination on the error's line and each one around it on a line of its own.
fn report(g: &G, source: &str, e: &Error) -> ExitCode {
    let mut err = Sink {
        w: io::stderr().lock(),
        error: None,
    };
    let _ = fmt::Write::write_fmt(&mut err, format_args!("{source}: "));
    let _ = g.describe(e, &mut err);
    let _ = fmt::Write::write_str(&mut err, "\n");
    ExitCode::FAILURE
}

/// `(report-host thunk)`: calls `thunk` as a run of its own. An error that ends it is reported as
/// any run's is, even one no `catch!` could stop, and the session goes on; `exit` or a pipe no one
/// reads ends it.
fn report_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [thunk] = args else {
        return Err(g.error("arity-error", args));
    };
    match g.call(r, thunk, &[]) {
        Err(e) if exit_status(g, &e).is_none() => {
            report(g, "stdin", &e);
            Ok(Value::Inert)
        }
        done => done,
    }
}

/// The host functions, bound by name where `runner.grift` is evaluated, which gives the program
/// those it should have.
const HOSTS: [(&str, HostFunction<Runner>); 19] = [
    ("display", display),
    ("display-error", display_error),
    ("environment-variable", environment_variable),
    ("wall-clock-nanoseconds", wall_clock_nanoseconds),
    ("elapsed-nanoseconds", elapsed_nanoseconds),
    ("exit", exit),
    ("open-host", open_host),
    ("create-host", create_host),
    ("read-line-host", read_line_host),
    ("read-all-host", read_all_host),
    ("read-bytes-host", read_bytes_host),
    ("write-host", write_host),
    ("close-host", close_host),
    ("forget-host", forget_host),
    ("run-host", run_host),
    ("report-host", report_host),
    ("read-forms", read_forms),
    ("position", position),
    ("resolve-host", resolve_host),
];

/// `(read-forms source text)`: the forms of the text, each combination with `source` and its line.
fn read_forms(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [source, text] = args else {
        return Err(g.error("arity-error", args));
    };
    let (Some(source), Some(text)) = (owned_string(g, source)?, owned_string(g, text)?) else {
        return Err(g.error("type-error", args));
    };
    g.read_in(&source, &text)
}

/// `(position form)`: the source and line of a combination `read-forms` or an evaluated text read,
/// as a list; `()` for any other value.
fn position(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [form] = args else {
        return Err(g.error("arity-error", args));
    };
    let Some((source, line)) = g.position(form)? else {
        return Ok(Value::EmptyList);
    };
    let source = source.to_owned();
    let source = g.string(&source)?;
    g.list(&[source, Value::Integer(i64::from(line))])
}

/// Evaluates `runner.grift` in a private context holding what it needs, so the operations it
/// makes look names up there, never where the program could redefine them. It counts as library
/// code from then on, so an error's trace has only the program's combinations. Gives what runs a
/// session at a terminal.
fn set_up(g: &mut G, r: &mut Runner, arguments: &[String], script: &str) -> Result<Value, Error> {
    let private = g.private_context()?;
    let bind = |g: &mut G, name: &str, value: Value| {
        let name = Value::Symbol(g.symbol(name)?);
        g.bind(&private, &name, value)
    };
    for (name, f) in HOSTS {
        let f = g.host_function(name, f)?;
        bind(g, name, f)?;
    }
    let mut list = Value::EmptyList;
    for a in arguments.iter().rev() {
        let a = g.string(a)?;
        list = g.pair(a, list)?;
    }
    bind(g, "command-line-arguments", list)?;
    let script = g.string(script)?;
    bind(g, "script-path", script)?;
    let top = g.top_level();
    bind(g, "top", top)?;
    let session = g.evaluate_in(r, &private, "runner.grift", RUNNER)?;
    g.seal_library();
    Ok(session)
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let script = args.next();
    let arguments: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
    let mut g: G = match Grift::new() {
        Ok(g) => g,
        Err(e) => {
            say(format_args!(
                "grift: the library failed to load ({} values)\n",
                e.values.len()
            ));
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
        started: Instant::now(),
        script: script.as_ref().map(PathBuf::from),
    };
    let script_path = script.as_ref().map(|s| s.to_string_lossy().into_owned());
    let set = set_up(
        &mut g,
        &mut r,
        &arguments,
        script_path.as_deref().unwrap_or_default(),
    );
    let session = match set {
        Ok(session) => session,
        Err(e) => return report(&g, "grift", &e),
    };
    let name = script_path.unwrap_or_else(|| String::from("stdin"));
    let done = if script.is_none() && io::stdin().is_terminal() {
        g.call(&mut r, &session, &[])
    } else {
        let text = match &script {
            Some(path) => std::fs::read_to_string(path),
            None => io::read_to_string(io::stdin()),
        };
        match text {
            Ok(text) => {
                let top = g.top_level();
                g.evaluate_in(&mut r, &top, &name, &text)
            }
            Err(e) => {
                let missing = e.kind() == io::ErrorKind::NotFound;
                let e = failed(&mut g, "file-error", &name, &e);
                let status = report(&g, "grift", &e);
                if missing {
                    say(format_args!("{USAGE}\n"));
                }
                return status;
            }
        }
    };
    match done {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => match exit_status(&g, &e) {
            Some(status) => ExitCode::from(status),
            None => report(&g, &name, &e),
        },
    }
}
