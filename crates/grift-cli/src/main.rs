//! The grift runner. `grift SCRIPT ARGUMENT...` runs the script, which gets the arguments. With no
//! script it reads the program from stdin; when stdin is a terminal `runner.grift` evaluates each
//! complete input and prints the result. A script path or argument that isn't UTF-8 is refused with
//! `encoding-error` before anything runs. Data goes to stdout, diagnostics to stderr. Exit status:
//! 0, 1 on a grift error (a script that can't be read, or failed output, is one), or what the
//! program gave `exit`. Errors shown at a terminal don't count: a session ends 0 unless the program
//! calls `exit`, input ends inside a form, or standard input or error fails or a program closes it,
//! which is the session's own error and ends it with 1. One the shell closed is `/dev/null` by
//! then: Rust's runtime opens it there before `main`. A write to a pipe no one reads, on any file, ends
//! the run as `exit` does, by a limit of kind `broken-pipe`, quietly and with 141, the status a
//! shell gives a program that SIGPIPE ended.
//!
//! For scripting, the program gets a small fixed set of operations: `display`, `display-error`,
//! `read-file`, `read-forms`, `position`, `standard-input`, `standard-output`, `standard-error`,
//! `open-file`, `create-file`, `run-process` and `load` (made by `runner.grift`),
//! `command-line-arguments`, `script-path`, `environment-variable`, `wall-clock-nanoseconds`,
//! `elapsed-nanoseconds` and `exit`.

use grift::{Error, Grift, HostFunction, Value};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const USAGE: &str = "usage: grift [SCRIPT [ARGUMENT...]]\n  with no SCRIPT, reads the program \
                     from stdin (use `rlwrap grift` for line editing)";

/// The setup of the operations on files, called with the host functions below.
const RUNNER: &str = include_str!("runner.grift");

/// A file a program opened: its path, and its stream.
struct Opened {
    path: String,
    file: Stream,
}

/// A file is being read, being written, or closed, which is for good.
enum Stream {
    Reading(Box<dyn BufRead>),
    Writing(Box<dyn Write>),
    Closed,
}

/// The runner's state: the files opened, standard input, error and output first, and when the run
/// started.
struct Runner {
    files: Vec<Opened>,
    started: Instant,
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
        Ok(Some(s)) => fmt::Write::write_str(&mut out, s).map_err(|_| g.error("output-error", &[])),
        _ => g.write(v, &mut out),
    });
    // The sink's own error first: it says what really failed, and `written` turns it into the
    // runner's kinds.
    let wrote = match (out.error, printed) {
        (Some(e), _) => Err(e),
        (None, Err(e)) => return Err(e),
        (None, Ok(())) => out.w.flush(),
    };
    written(g, path, wrote)
}

/// `(display v ...)`: strings without quotes, everything else as written.
fn display(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    put(g, r, STANDARD_OUTPUT, args)
}

/// `(display-error v ...)`: as `display`, to stderr.
fn display_error(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    put(g, r, STANDARD_ERROR, args)
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

/// The arguments of a host function that takes exactly `N`; `arity-error` for any other count.
fn exactly<'a, const N: usize>(g: &mut G, args: &'a [Value]) -> Result<&'a [Value; N], Error> {
    args.try_into().map_err(|_| g.error("arity-error", args))
}

/// A duration as an integer of nanoseconds; `range-error` before 1970 or after 2262.
fn nanoseconds(g: &mut G, args: &[Value], d: Option<Duration>) -> Result<Value, Error> {
    let [] = exactly(g, args)?;
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
        return Err(limit(g, "out-of-memory", &[]));
    }
    copy.push_str(g.as_str(v)?.unwrap_or_default());
    Ok(Some(copy))
}

/// The one string argument, or a `type-error` or an `arity-error`.
fn one_string(g: &mut G, args: &[Value]) -> Result<String, Error> {
    let [v] = exactly(g, args)?;
    match owned_string(g, v)? {
        Some(s) => Ok(s),
        None => Err(g.error("type-error", args)),
    }
}

/// A limit of `kind`: the runner's are `out-of-memory`, `broken-pipe` and `exit`.
fn limit(g: &mut G, kind: &str, values: &[Value]) -> Error {
    Error {
        limit: true,
        ..g.error(kind, values)
    }
}

/// An I/O error about `what` (a path or a program): out of memory, `encoding-error` for bytes that
/// aren't UTF-8, else `kind` with the system's message.
fn failed(g: &mut G, kind: &str, what: &str, e: &io::Error) -> Error {
    if e.kind() == io::ErrorKind::OutOfMemory {
        return limit(g, "out-of-memory", &[]);
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
            let id = i64::try_from(r.files.len()).map_err(|_| limit(g, "out-of-memory", &[]))?;
            r.files.push(Opened { path, file });
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
            file: Stream::Closed,
        }) => {
            let path = g.string(path)?;
            Err(g.error("closed", &[path]))
        }
        Some(Opened { path, file }) => Ok((path, file)),
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
    let [id] = exactly(g, args)?;
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
    let [id, count] = exactly(g, args)?;
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
            return Err(limit(g, "out-of-memory", &[]));
        }
        bytes.extend_from_slice(chunk);
        let used = chunk.len();
        input.consume(used);
    }
    g.bytes(&bytes)
}

/// `(write data)` on a file: all of the bytes of a string or a bytes value.
fn write_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, data] = exactly(g, args)?;
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
    written(g, path, wrote)
}

/// After a write to the file `path`: `#inert`, or a `file-error`, except that a pipe no one reads
/// is the limit that ends the run quietly, as [`exit_status`] says, and only a real write makes it.
fn written(g: &mut G, path: &str, wrote: io::Result<()>) -> Result<Value, Error> {
    match wrote {
        Ok(()) => Ok(Value::Inert),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
            let path = g.string(path)?;
            Err(limit(g, "broken-pipe", &[path]))
        }
        Err(e) => Err(failed(g, "file-error", path, &e)),
    }
}

/// `(close)` on a file: closes it; using it after this is the error `closed`.
fn close_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id] = exactly(g, args)?;
    *file(g, r, id)?.1 = Stream::Closed;
    Ok(Value::Inert)
}

/// `(read-file path)`: the file's whole text.
fn read_file(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let path = one_string(g, args)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => g.string(&text),
        Err(e) => Err(failed(g, "file-error", &path, &e)),
    }
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
            Err(limit(g, "exit", args))
        }
        [_] => Err(g.error("type-error", args)),
        _ => Err(g.error("arity-error", args)),
    }
}

/// The status a run ended with by the runner's own limits, which a program can't begin: what `exit`
/// gave, or 141 for a pipe no one reads, as a shell says of a program SIGPIPE ended: `grift script |
/// head` is quiet, and a run that ended this way never reports success.
fn exit_status(g: &G, e: &Error) -> Option<u8> {
    if !e.limit {
        return None;
    }
    match (g.symbol_name(&e.kind).ok()?, e.values.as_slice()) {
        ("broken-pipe", _) => Some(141),
        ("exit", [Value::Integer(n)]) => u8::try_from(*n).ok(),
        _ => None,
    }
}

/// Runs `program` to its end with no input: its exit status, then what it wrote to stderr and to
/// stdout, each `None` if memory was refused for it. A thread reads stderr while this one reads
/// stdout, so neither pipe fills and blocks the program.
fn ran(program: &str, arguments: &[String]) -> io::Result<(ExitStatus, [Option<Vec<u8>>; 2])> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(io::Error::other("no pipe"));
    };
    let read_stderr = match thread::Builder::new().spawn(move || drain(stderr)) {
        Ok(thread) => thread,
        Err(e) => {
            let _ = child.kill();
            child.wait()?;
            return Err(e);
        }
    };
    let stdout = drain(stdout);
    let stderr = read_stderr
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("panic")));
    let status = child.wait()?;
    Ok((status, [stderr?, stdout?]))
}

/// Reads `pipe` to its end: all of it, or `None` if memory was refused, the rest read and dropped.
fn drain(mut pipe: impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut kept = Some(Vec::new());
    let mut chunk = [0; 8192];
    loop {
        let n = match pipe.read(&mut chunk) {
            Ok(0) => return Ok(kept),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if let Some(bytes) = &mut kept {
            match (bytes.try_reserve(n), chunk.get(..n)) {
                (Ok(()), Some(read)) => bytes.extend_from_slice(read),
                _ => kept = None,
            }
        }
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
    let (status, outputs) = match ran(program, arguments) {
        Ok(ran) => ran,
        Err(e) => return Err(failed(g, "process-error", program, &e)),
    };
    let status = status
        .code()
        .map_or(Value::Inert, |n| Value::Integer(i64::from(n)));
    // Refused memory is the limit, whichever output it was refused to, before either is text.
    let [Some(stderr), Some(stdout)] = outputs else {
        return Err(limit(g, "out-of-memory", &[]));
    };
    let mut list = Value::EmptyList;
    for bytes in [stderr, stdout] {
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
/// A report up to 4 KiB is one write, so output sharing its pipe can't land inside it.
fn report(g: &G, source: &str, e: &Error) -> ExitCode {
    let mut err = Sink {
        w: Gathered {
            out: io::stderr().lock(),
            text: [0; 4096],
            len: 0,
        },
        error: None,
    };
    let _ = fmt::Write::write_fmt(&mut err, format_args!("{source}: "));
    let _ = g.describe(e, &mut err);
    let _ = fmt::Write::write_str(&mut err, "\n");
    let _ = err.w.flush();
    ExitCode::FAILURE
}

/// Writes gathered on the stack and passed on when it is full or flushed: no memory to be refused.
struct Gathered<W: Write> {
    out: W,
    text: [u8; 4096],
    len: usize,
}

impl<W: Write> Write for Gathered<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.len == self.text.len() {
            self.flush()?;
        }
        let mut room = self.text.get_mut(self.len..).unwrap_or_default();
        let n = room.write(data)?;
        self.len += n;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        let gathered = self.text.get(..self.len).unwrap_or_default();
        self.len = 0;
        self.out.write_all(gathered)?;
        self.out.flush()
    }
}

/// `(report-host thunk)`: calls `thunk` as a run of its own. An error that ends it is reported as
/// any run's is, even one no `catch!` could stop, and the session goes on; `exit` or a pipe no one
/// reads ends it.
fn report_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [thunk] = exactly(g, args)?;
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
const HOSTS: [(&str, HostFunction<Runner>); 18] = [
    ("display", display),
    ("display-error", display_error),
    ("read-file", read_file),
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
    ("run-host", run_host),
    ("report-host", report_host),
    ("read-forms", read_forms),
    ("position", position),
];

/// `(read-forms source text)`: the forms of the text, each combination with `source` and its line.
fn read_forms(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [source, text] = exactly(g, args)?;
    let (Some(source), Some(text)) = (owned_string(g, source)?, owned_string(g, text)?) else {
        return Err(g.error("type-error", args));
    };
    g.read_in(&source, &text)
}

/// `(position form)`: the source and line of a combination `read-forms` or an evaluated text read,
/// as a list; `()` for any other value.
fn position(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [form] = exactly(g, args)?;
    let Some((source, line)) = g.position(form)? else {
        return Ok(Value::EmptyList);
    };
    let source = source.to_owned();
    let source = g.string(&source)?;
    let line = g.pair(Value::Integer(i64::from(line)), Value::EmptyList)?;
    g.pair(source, line)
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
    g.evaluate_library_in(r, &private, "runner.grift", RUNNER)
}

fn main() -> ExitCode {
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
    let mut words = Vec::new();
    for word in std::env::args_os().skip(1) {
        match word.into_string() {
            Ok(word) => words.push(word),
            Err(word) => {
                let e = match g.bytes(word.as_encoded_bytes()) {
                    Ok(bytes) => g.error("encoding-error", &[bytes]),
                    Err(e) => e,
                };
                return report(&g, "grift", &e);
            }
        }
    }
    let (script, arguments) = match words.split_first() {
        Some((script, arguments)) => (Some(script.as_str()), arguments),
        None => (None, &[][..]),
    };
    let mut r = Runner {
        files: vec![
            Opened {
                path: String::from("standard-input"),
                file: Stream::Reading(Box::new(BufReader::new(io::stdin()))),
            },
            Opened {
                path: String::from("standard-error"),
                file: Stream::Writing(Box::new(io::stderr())),
            },
            Opened {
                path: String::from("standard-output"),
                file: Stream::Writing(Box::new(io::stdout())),
            },
        ],
        started: Instant::now(),
    };
    let set = set_up(&mut g, &mut r, arguments, script.unwrap_or_default());
    let session = match set {
        Ok(session) => session,
        Err(e) => return report(&g, "grift", &e),
    };
    let name = script.unwrap_or("stdin");
    let done = if script.is_none() && io::stdin().is_terminal() {
        g.call(&mut r, &session, &[])
    } else {
        let text = match script {
            Some(path) => std::fs::read_to_string(path),
            None => io::read_to_string(io::stdin()),
        };
        match text {
            Ok(text) => {
                let top = g.top_level();
                g.evaluate_in(&mut r, &top, name, &text)
            }
            Err(e) => {
                let missing = e.kind() == io::ErrorKind::NotFound;
                let e = failed(&mut g, "file-error", name, &e);
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
            None => report(&g, name, &e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::Gathered;
    use std::io::{self, Write};

    /// What was written, and in how many writes.
    #[derive(Default)]
    struct Counted {
        data: Vec<u8>,
        writes: usize,
    }

    impl Write for Counted {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            self.data.extend_from_slice(data);
            Ok(data.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A report up to 4 KiB, written in pieces, reaches stderr in one write, so output sharing its
    /// pipe can't land inside it.
    #[test]
    fn a_report_is_one_write() {
        let mut w = Gathered {
            out: Counted::default(),
            text: [0; 4096],
            len: 0,
        };
        for piece in ["stdin: ", "type-error", " 1", "\n"] {
            w.write_all(piece.as_bytes()).unwrap();
        }
        w.flush().unwrap();
        assert_eq!(
            (&w.out.data[..], w.out.writes),
            (&b"stdin: type-error 1\n"[..], 1)
        );
    }
}
