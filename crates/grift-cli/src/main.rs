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

use grift::{Answer, Error, Grift, HostFunction, Ran, ReportStatus, Value};
use std::fmt::{self, Write as _};
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const USAGE: &str = "usage: grift [SCRIPT [ARGUMENT...]]\n  with no SCRIPT, reads the program \
                     from stdin (use `rlwrap grift` for line editing)";

/// The setup of the operations on files, called with the host functions below.
const RUNNER: &str = include_str!("runner.grift");

/// A file is being read or written.
enum Stream {
    Reading(Box<dyn BufRead>),
    Writing(Box<dyn Write>),
}

/// The runner's state: the files open, by their numbers, which only grow: standard input, error
/// and output first; the next file's number; and when the run started. A file closed leaves
/// `files`, so it holds only what is open, and a number below `next` not in it is closed for good.
struct Runner {
    files: Vec<(i64, Stream)>,
    next: i64,
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

/// The numbers and names of the standard streams that are written to.
const STANDARD_ERROR: (i64, &str) = (1, "standard-error");
const STANDARD_OUTPUT: (i64, &str) = (2, "standard-output");

/// Writes the values to file `id`, strings without quotes and everything else as written, and
/// flushes.
fn put(
    g: &mut G,
    r: &mut Runner,
    (id, path): (i64, &str),
    values: &[Value],
) -> Result<Value, Error> {
    let found = place(r, &Value::Integer(id))
        .ok()
        .and_then(|i| r.files.get_mut(i));
    let Some((_, Stream::Writing(w))) = found else {
        let path = g.string(path)?;
        return Err(g.error("closed", &[path]));
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
    if wrote.is_ok() {
        return Ok(Value::Inert);
    }
    let path = g.string(path)?;
    written(g, &path, wrote)
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
    let Some(s) = g.as_str(v)? else {
        return Ok(None);
    };
    match copy(s) {
        Some(s) => Ok(Some(s)),
        None => Err(limit(g, "out-of-memory", &[])),
    }
}

/// A copy of `s`, `None` if memory was refused for it.
fn copy(s: &str) -> Option<String> {
    let mut copy = String::new();
    copy.try_reserve_exact(s.len()).ok()?;
    copy.push_str(s);
    Some(copy)
}

/// The most bytes of paths, names and a process's words one operation hands the system, which
/// copies them where refused memory aborts: more is a `range-error`.
const SYSTEM_BYTES: usize = 1 << 20;

/// The one string argument, a path or a name for the system; or a `type-error`, an `arity-error`
/// or a `range-error`.
fn one_string(g: &mut G, args: &[Value]) -> Result<String, Error> {
    let [v] = exactly(g, args)?;
    match owned_string(g, v)? {
        Some(s) if s.len() <= SYSTEM_BYTES => Ok(s),
        Some(_) => Err(g.error("range-error", args)),
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
fn failed(g: &mut G, kind: &str, what: &Value, e: &io::Error) -> Error {
    let what = what.clone();
    let mut message = grift::TextBuffer::new();
    if e.kind() == io::ErrorKind::OutOfMemory || write!(message, "{e}").is_err() {
        return limit(g, "out-of-memory", &[]);
    }
    if e.kind() == io::ErrorKind::InvalidData {
        return g.error("encoding-error", &[what]);
    }
    match g.string(message.as_str()) {
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
    let (Ok(()), Some(next)) = (r.files.try_reserve(1), r.next.checked_add(1)) else {
        return Err(limit(g, "out-of-memory", &[]));
    };
    let what = args.first().unwrap_or(&Value::Inert);
    let file = open(&path).map_err(|e| failed(g, "file-error", what, &e))?;
    r.files.push((r.next, file));
    r.next = next;
    Ok(Value::Integer(next - 1))
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

/// Where the open file whose number is `id` is in `Runner::files`: `Err(true)` once it is closed,
/// `Err(false)` if no file had that number.
fn place(r: &Runner, id: &Value) -> Result<usize, bool> {
    let &Value::Integer(n) = id else {
        return Err(false);
    };
    r.files
        .binary_search_by_key(&n, |(id, _)| *id)
        .map_err(|_| (0..r.next).contains(&n))
}

/// The open file whose number is `id`, its place and stream; `closed` with its `path` once it is
/// closed.
fn file<'r>(
    g: &mut G,
    r: &'r mut Runner,
    id: &Value,
    path: &Value,
) -> Result<(usize, &'r mut Stream), Error> {
    match place(r, id).map(|i| (i, r.files.get_mut(i))) {
        Ok((i, Some((_, file)))) => Ok((i, file)),
        Err(true) => Err(g.error("closed", std::slice::from_ref(path))),
        _ => Err(g.error("type-error", std::slice::from_ref(id))),
    }
}

/// The next bytes of the file whose number and path are `file`, `most` of them at most and, for a
/// `line`, up to its newline; none at the end.
fn gather(
    g: &mut G,
    r: &mut Runner,
    file: [&Value; 2],
    most: u64,
    line: bool,
) -> Result<Vec<u8>, Error> {
    let [id, path] = file;
    let (_, Stream::Reading(input)) = self::file(g, r, id, path)? else {
        return Err(g.error("type-error", std::slice::from_ref(id)));
    };
    let mut input = input.as_mut().take(most);
    let mut bytes = Vec::new();
    loop {
        let chunk = match input.fill_buf() {
            Ok([]) => break,
            Ok(chunk) => chunk,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(failed(g, "file-error", path, &e)),
        };
        let newline = chunk.iter().position(|&b| line && b == b'\n');
        let used = chunk
            .get(..newline.map_or(chunk.len(), |i| i + 1))
            .unwrap_or_default();
        if bytes.try_reserve(used.len()).is_err() {
            return Err(limit(g, "out-of-memory", &[]));
        }
        bytes.extend_from_slice(used);
        let used = used.len();
        input.consume(used);
        if newline.is_some() {
            break;
        }
    }
    Ok(bytes)
}

/// What `gather` gives of the file, as text; an `encoding-error` if it isn't UTF-8.
fn text(g: &mut G, r: &mut Runner, args: &[Value], line: bool) -> Result<Value, Error> {
    let [id, path] = exactly(g, args)?;
    let bytes = gather(g, r, [id, path], u64::MAX, line)?;
    match std::str::from_utf8(&bytes) {
        Ok(text) => g.string(text),
        Err(_) => Err(g.error("encoding-error", std::slice::from_ref(path))),
    }
}

/// `(read-line)` on a file: the next line with its newline, `""` at the end.
fn read_line_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    text(g, r, args, true)
}

/// `(read-all)` on a file: the rest of it.
fn read_all_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    text(g, r, args, false)
}

/// `(read-bytes count)` on a file: the next `count` bytes, fewer at the end, as a bytes value;
/// `#bytes()` at the end. It reads only as much as it gives, so a pipe that stays open answers as
/// soon as `count` bytes have come.
fn read_bytes_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, path, count] = exactly(g, args)?;
    let &Value::Integer(n) = count else {
        return Err(g.error("type-error", std::slice::from_ref(count)));
    };
    let n = u64::try_from(n).map_err(|_| g.error("range-error", std::slice::from_ref(count)))?;
    let bytes = gather(g, r, [id, path], n, false)?;
    g.bytes(&bytes)
}

/// `(write data)` on a file: all of the bytes of a string or a bytes value.
fn write_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, path, data] = exactly(g, args)?;
    let (_, Stream::Writing(out)) = file(g, r, id, path)? else {
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
fn written(g: &mut G, path: &Value, wrote: io::Result<()>) -> Result<Value, Error> {
    match wrote {
        Ok(()) => Ok(Value::Inert),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
            Err(limit(g, "broken-pipe", std::slice::from_ref(path)))
        }
        Err(e) => Err(failed(g, "file-error", path, &e)),
    }
}

/// `(close)` on a file: closes it; using it after this is the error `closed`.
fn close_host(g: &mut G, r: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let [id, path] = exactly(g, args)?;
    let (i, _) = file(g, r, id, path)?;
    r.files.remove(i);
    Ok(Value::Inert)
}

/// `(read-file path)`: the file's whole text.
fn read_file(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Value, Error> {
    let path = one_string(g, args)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => g.string(&text),
        Err(e) => Err(failed(
            g,
            "file-error",
            args.first().unwrap_or(&Value::Inert),
            &e,
        )),
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
    let [status] = exactly(g, args)?;
    let &Value::Integer(n) = status else {
        return Err(g.error("type-error", args));
    };
    u8::try_from(n).map_err(|_| g.error("range-error", args))?;
    Err(limit(g, "exit", args))
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
    if words.try_reserve_exact(args.len()).is_err() {
        return Err(limit(g, "out-of-memory", &[]));
    }
    for v in args {
        match owned_string(g, v)? {
            Some(s) => words.push(s),
            None => return Err(g.error("type-error", std::slice::from_ref(v))),
        }
    }
    // Each word also costs the system its end byte and a pointer to it, so empty words count too.
    let size = |w: &String| w.len() + 1 + size_of::<usize>();
    if words.iter().map(size).sum::<usize>() > SYSTEM_BYTES {
        return Err(g.error("range-error", args));
    }
    let (Some((program, arguments)), Some(name)) = (words.split_first(), args.first()) else {
        return Err(g.error("arity-error", args));
    };
    let (status, outputs) = match ran(program, arguments) {
        Ok(ran) => ran,
        Err(e) => return Err(failed(g, "process-error", name, &e)),
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

/// Prints an error to stderr as its source, then its kind, where it happened and its values, the
/// innermost combination on the error's line and each one around it on a line of its own. A
/// report longer than [`REPORT_BYTES`], or one leaving deep data out, is cut short, and then a
/// line of the runner's own says so: no text of the program's can begin a line in a report. A
/// report up to 4 KiB is one write, so output sharing its pipe can't land inside it.
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
    if let Ok(ReportStatus::Truncated) = g.describe(e, &mut err, REPORT_BYTES) {
        let _ = fmt::Write::write_str(&mut err, "\n  (report cut short)");
    }
    let _ = fmt::Write::write_str(&mut err, "\n");
    let _ = err.w.flush();
    ExitCode::FAILURE
}

/// The most a report of an error takes, before the line saying it was cut.
const REPORT_BYTES: usize = 4096;

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

/// `(report-host thunk)`: has the host's loop call `thunk` as a run of its own, so an error that
/// ends it, even one no `catch!` could stop, is reported as any run's is and the session goes on:
/// the session's run waits for it, in [`run_session`].
fn report_host(g: &mut G, _: &mut Runner, args: &[Value]) -> Result<Answer, Error> {
    let [thunk] = exactly(g, args)?;
    Ok(Answer::Later(thunk.clone()))
}

/// Runs the terminal session, a run that waits at each `report-host` while its thunk runs here,
/// as a run of its own, whose ending is the answer; `exit` or a pipe no one reads ends it.
fn run_session(g: &mut G, r: &mut Runner, session: &Value) -> Result<Value, Error> {
    let mut ran = g.start(r, session, &[])?;
    loop {
        let parked = match ran {
            Ran::Finished(v) => return Ok(v),
            Ran::Parked(parked) => parked,
        };
        let thunk = parked.request().clone();
        let answer = match g.call(r, &thunk, &[]) {
            Err(e) if exit_status(g, &e).is_none() => {
                report(g, "stdin", &e);
                Ok(Value::Inert)
            }
            done => done,
        };
        ran = g.resume(r, parked, answer)?;
    }
}

/// A host function that answers at once, with the value `f` gives.
macro_rules! now {
    ($f:ident) => {
        |g, r, args| $f(g, r, args).map(Answer::Now)
    };
}

/// The host functions, bound by name where `runner.grift` is evaluated, which gives the program
/// those it should have.
const HOSTS: [(&str, HostFunction<Runner>); 18] = [
    ("display", now!(display)),
    ("display-error", now!(display_error)),
    ("read-file", now!(read_file)),
    ("environment-variable", now!(environment_variable)),
    ("wall-clock-nanoseconds", now!(wall_clock_nanoseconds)),
    ("elapsed-nanoseconds", now!(elapsed_nanoseconds)),
    ("exit", now!(exit)),
    ("open-host", now!(open_host)),
    ("create-host", now!(create_host)),
    ("read-line-host", now!(read_line_host)),
    ("read-all-host", now!(read_all_host)),
    ("read-bytes-host", now!(read_bytes_host)),
    ("write-host", now!(write_host)),
    ("close-host", now!(close_host)),
    ("run-host", now!(run_host)),
    ("report-host", report_host),
    ("read-forms", now!(read_forms)),
    ("position", now!(position)),
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
    let Some(source) = copy(source) else {
        return Err(limit(g, "out-of-memory", &[]));
    };
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
    let script = words.first().map(String::as_str);
    let arguments = words.get(1..).unwrap_or_default();
    let mut r = Runner {
        files: vec![
            (0, Stream::Reading(Box::new(BufReader::new(io::stdin())))),
            (1, Stream::Writing(Box::new(io::stderr()))),
            (2, Stream::Writing(Box::new(io::stdout()))),
        ],
        next: 3,
        started: Instant::now(),
    };
    let session = match set_up(&mut g, &mut r, arguments, script.unwrap_or_default()) {
        Ok(session) => session,
        Err(e) => return report(&g, "grift", &e),
    };
    let name = script.unwrap_or("stdin");
    let done = if script.is_none() && io::stdin().is_terminal() {
        run_session(&mut g, &mut r, &session)
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
                let e = match g.string(name) {
                    Ok(name) => failed(&mut g, "file-error", &name, &e),
                    Err(e) => e,
                };
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
