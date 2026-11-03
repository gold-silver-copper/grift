//! An Emacs-style editor whose editor is grift code. The host gives grift bytes and files only:
//! a key as a byte, writes to the terminal, the terminal's size, reading and writing a file. The
//! buffer, redisplay, the keymap, the minibuffer and every command live in `editor.grift`.
//!
//! A thread reads the terminal while grift runs, so C-g stops a command that never returns: it
//! holds an `Interrupter`, and the command ends with the error `interrupted`.
//!
//! `grift-emacs [--init INIT.grift] [FILE]`

use grift::{Error, Grift, HostFunction, Value};
use std::fmt;
use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

const EDITOR: &str = include_str!("editor.grift");
const USAGE: &str = "usage: grift-emacs [--init INIT.grift] [FILE]";

/// C-g, the byte that interrupts.
const QUIT: u8 = 7;

/// The longest error text shown: an error may carry a whole buffer.
const ERROR_TEXT_MAX: usize = 400;

/// What the host functions work on.
struct Host {
    /// Terminal output waiting for the next key read.
    out: Vec<u8>,
    /// The bytes typed, from the input thread, each with whether it stopped the command.
    keys: Receiver<(u8, bool)>,
    /// Set while `(key)` waits, so a C-g then is a key, not a stop.
    waiting: Arc<AtomicBool>,
    /// How many keys grift has read.
    read: u64,
}

impl Host {
    fn flush(&mut self) {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(&self.out).and_then(|()| stdout.flush());
        self.out.clear();
    }
}

type G = Grift<Host>;

/// `(key)`: the next byte from the terminal, after showing what was written. A C-g typed while
/// this waits is a key. One typed while a command ran stopped it: if it comes here, the stop
/// asked for is this run's, so it ends now with `interrupted`.
fn key(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    h.flush();
    h.waiting.store(true, Ordering::SeqCst);
    let got = h.keys.recv();
    h.waiting.store(false, Ordering::SeqCst);
    match got {
        Ok((_, true)) => Err(g.error("interrupted", &[])),
        Ok((b, false)) => {
            h.read += 1;
            Ok(Value::Integer(i64::from(b)))
        }
        Err(_) => Err(g.error("end-of-input", &[])),
    }
}

/// `(terminal-write string ...)`: queues the strings for the terminal.
fn terminal_write(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    for v in args {
        match g.as_str(v)? {
            Some(s) => h.out.extend_from_slice(s.as_bytes()),
            None => return Err(g.error("type-error", std::slice::from_ref(v))),
        }
    }
    Ok(Value::Inert)
}

/// `(terminal-size)`: the terminal's rows and columns, as a list.
fn terminal_size(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cols = g.pair(Value::Integer(i64::from(cols)), Value::EmptyList)?;
    g.pair(Value::Integer(i64::from(rows)), cols)
}

fn file_error(g: &mut G, path: &Value, e: &io::Error) -> Error {
    match g.string(&e.to_string()) {
        Ok(why) => g.error("file-error", &[path.clone(), why]),
        Err(e) => e,
    }
}

/// `(read-file path)`: the file's text; `()` when it doesn't exist.
fn read_file(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [path] = args else {
        return Err(g.error("arity-error", args));
    };
    let Some(name) = g.as_string(path)? else {
        return Err(g.error("type-error", args));
    };
    match std::fs::read_to_string(&name) {
        Ok(s) => g.string(&s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Value::EmptyList),
        Err(e) => Err(file_error(g, path, &e)),
    }
}

/// `(write-file path string)`: replaces the file's contents with the string's bytes.
fn write_file(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [path, contents] = args else {
        return Err(g.error("arity-error", args));
    };
    let (Some(name), Some(s)) = (g.as_str(path)?, g.as_str(contents)?) else {
        return Err(g.error("type-error", args));
    };
    match std::fs::write(name, s) {
        Ok(()) => Ok(Value::Inert),
        Err(e) => Err(file_error(g, path, &e)),
    }
}

/// A `fmt::Write` into a string of one line that stops at `ERROR_TEXT_MAX` bytes.
struct Capped(String);

impl fmt::Write for Capped {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if self.0.len() + c.len_utf8() > ERROR_TEXT_MAX {
                self.0.push_str("...");
                return Err(fmt::Error);
            }
            self.0.push(if c == '\n' { ' ' } else { c });
        }
        Ok(())
    }
}

/// An error as its kind, values and trace, written out now: the values may be collected by
/// the next `evaluate`.
fn describe(g: &G, e: &Error) -> String {
    let mut out = Capped(String::new());
    let _ = g.describe(e, &mut out);
    out.0
}

/// Calls the editor's function `name` with string arguments (`None` for `()`).
fn enter(g: &mut G, h: &mut Host, name: &str, args: &[Option<&str>]) -> Result<Value, Error> {
    let f = g.evaluate(h, name)?;
    let mut values = Vec::new();
    for a in args {
        values.push(match a {
            Some(s) => g.string(s)?,
            None => Value::EmptyList,
        });
    }
    g.call(h, &f, &values)
}

const HOST: [(&str, HostFunction<Host>); 5] = [
    ("key", key),
    ("terminal-write", terminal_write),
    ("terminal-size", terminal_size),
    ("read-file", read_file),
    ("write-file", write_file),
];

/// Runs the editor until it quits or input ends. An error before the first key (the file can't
/// be read) ends it. After every later error the editor goes on through `(editor-recover
/// text)`. If the editor's own code fails twice with no key read in between, the host asks
/// before trying again.
fn run(g: &mut G, h: &mut Host, file: Option<&str>, init: Option<&str>) -> Result<(), String> {
    let mut keys = h.read;
    let mut entered = enter(g, h, "editor-main", &[file, init]);
    loop {
        let e = match entered {
            Ok(_) => return Ok(()),
            Err(e) if g.is_kind(&e, "end-of-input").unwrap_or(false) => return Ok(()),
            Err(e) => e,
        };
        let text = describe(g, &e);
        if h.read == 0 {
            return Err(text);
        }
        if h.read == keys {
            let mut out = io::stdout().lock();
            let _ = write!(
                out,
                "\x1b[999;1H\x1b[0m\x1b[K{text} -- the editor itself fails: q quits, any other key retries"
            );
            let _ = out.flush();
            if !matches!(h.keys.recv(), Ok((b, _)) if b != b'q') {
                return Ok(());
            }
        }
        keys = h.read;
        entered = enter(g, h, "editor-recover", &[Some(&text)]);
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut init, mut file) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--init" if init.is_none() => match args.next() {
                Some(path) => init = Some(path),
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            _ if a.starts_with('-') || file.is_some() => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            _ => file = Some(a),
        }
    }
    let init = match init.map(std::fs::read_to_string).transpose() {
        Ok(init) => init,
        Err(e) => {
            eprintln!("grift-emacs: --init: {e}");
            return ExitCode::from(2);
        }
    };
    let (send, keys) = mpsc::channel();
    let waiting = Arc::new(AtomicBool::new(false));
    let mut h = Host {
        out: Vec::new(),
        keys,
        waiting: Arc::clone(&waiting),
        read: 0,
    };
    let mut g: G = match Grift::new() {
        Ok(g) => g,
        Err(_) => {
            eprintln!("grift-emacs: grift failed to start");
            return ExitCode::FAILURE;
        }
    };
    let mut stop = g.interrupter();
    for (name, f) in HOST {
        if g
            .host_function(name, f)
            .and_then(|f| g.define(name, f))
            .is_err()
        {
            eprintln!("grift-emacs: out of memory");
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = g.evaluate(&mut h, EDITOR) {
        eprintln!("grift-emacs: editor.grift: {}", describe(&g, &e));
        return ExitCode::FAILURE;
    }
    // An init file that fails is reported in the echo area, not fatal.
    let init = match init.map(|src| g.evaluate(&mut h, &src)) {
        Some(Err(e)) => format!("init: {}", describe(&g, &e)),
        _ => String::new(),
    };
    if let Err(e) = crossterm::terminal::enable_raw_mode() {
        eprintln!("grift-emacs: the terminal: {e}");
        return ExitCode::FAILURE;
    }
    // The input thread: every byte typed goes to `(key)`; a C-g typed while no key is awaited
    // also stops the command running. It ends with the input, and when the program exits.
    std::thread::spawn(move || {
        let mut b = [0u8];
        while let Ok(1) = io::stdin().lock().read(&mut b) {
            let stopped = b[0] == QUIT && !waiting.load(Ordering::SeqCst);
            if stopped {
                stop.stop();
            }
            if send.send((b[0], stopped)).is_err() {
                break;
            }
        }
    });
    // The alternate screen, with lines clipped at the right edge instead of wrapped.
    print!("\x1b[?1049h\x1b[?7l");
    let ran = run(&mut g, &mut h, file.as_deref(), Some(&init));
    h.flush();
    print!("\x1b[?7h\x1b[?1049l");
    let _ = io::stdout().flush();
    let _ = crossterm::terminal::disable_raw_mode();
    match ran {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("grift-emacs: {e}");
            ExitCode::FAILURE
        }
    }
}
