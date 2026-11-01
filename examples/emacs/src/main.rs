//! An Emacs-style editor whose editor is grift code. The host gives grift bytes and files only:
//! a key as a byte, writes to the terminal, the terminal's size, reading and writing a file,
//! the conversions between strings and grift data, and the reader. The buffer, redisplay, the
//! keymap, the minibuffer and every command live in `editor.grift`.
//!
//! A thread reads the terminal while grift runs, so C-g stops a command that never returns:
//! it sets grift's interrupt flag, and the command ends with the error `interrupted`.
//!
//! `grift-emacs [--init INIT.grift] [FILE]`

use grift::{Error, Grift, Value};
use std::fmt;
use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

const EDITOR: &str = include_str!("editor.grift");
const USAGE: &str = "usage: grift-emacs [--init INIT.grift] [FILE]";

/// C-g, the byte that interrupts.
const QUIT: u8 = 7;

/// Grift's interrupt flag: set by the input thread when C-g arrives.
static STOP: AtomicBool = AtomicBool::new(false);

/// The longest error text shown: an error may carry a whole buffer.
const ERROR_TEXT_MAX: usize = 400;

/// What the host functions work on.
struct Host {
    /// Terminal output waiting for the next key read.
    out: Vec<u8>,
    /// The bytes typed, from the input thread.
    keys: Receiver<u8>,
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

fn text(g: &G, v: Value) -> Option<String> {
    g.string(v).map(String::from)
}

/// `(key)`: the next byte from the terminal, after showing what was written. The input thread
/// sets the interrupt flag before it passes a C-g on. A C-g that finds the flag still set
/// arrived while grift waited here: it is a key, and the flag is cleared. One that finds it
/// clear has already interrupted a command, which cleared it: it is dropped.
fn key(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    h.flush();
    loop {
        let Ok(b) = h.keys.recv() else {
            return Err(g.error("end-of-input", &[]));
        };
        if b != QUIT || STOP.swap(false, Ordering::Relaxed) {
            h.read += 1;
            return Ok(Value::Int(i64::from(b)));
        }
    }
}

/// `(term-write string ...)`: queues the strings for the terminal.
fn term_write(g: &mut G, h: &mut Host, args: &[Value]) -> Result<Value, Error> {
    for &v in args {
        match g.string(v) {
            Some(s) => h.out.extend_from_slice(s.as_bytes()),
            None => return Err(g.error("type-error", &[v])),
        }
    }
    Ok(Value::Inert)
}

/// `(term-size)`: the terminal's rows and columns, as a list.
fn term_size(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    if !args.is_empty() {
        return Err(g.error("arity-error", args));
    }
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cols = g.cons(Value::Int(i64::from(cols)), Value::Nil)?;
    g.cons(Value::Int(i64::from(rows)), cols)
}

fn file_error(g: &mut G, path: Value, e: &io::Error) -> Error {
    match g.make_string(&e.to_string()) {
        Ok(why) => g.error("file-error", &[path, why]),
        Err(e) => e,
    }
}

/// `(read-file path)`: the file's text; `()` when it doesn't exist.
fn read_file(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [path] = args else {
        return Err(g.error("arity-error", args));
    };
    let Some(name) = text(g, *path) else {
        return Err(g.error("type-error", args));
    };
    match std::fs::read_to_string(&name) {
        Ok(s) => g.make_string(&s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Value::Nil),
        Err(e) => Err(file_error(g, *path, &e)),
    }
}

/// `(write-file path string)`: replaces the file's contents with the string's bytes.
fn write_file(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [path, contents] = args else {
        return Err(g.error("arity-error", args));
    };
    let (Some(name), Some(s)) = (g.string(*path), g.string(*contents)) else {
        return Err(g.error("type-error", args));
    };
    match std::fs::write(name, s) {
        Ok(()) => Ok(Value::Inert),
        Err(e) => Err(file_error(g, *path, &e)),
    }
}

/// `(string->codes s)`: the string's characters as a list of code points.
fn string_to_codes(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [s] = args else {
        return Err(g.error("arity-error", args));
    };
    let Some(s) = text(g, *s) else {
        return Err(g.error("type-error", args));
    };
    let mut list = Value::Nil;
    for c in s.chars().rev() {
        list = g.cons(Value::Int(i64::from(u32::from(c))), list)?;
    }
    Ok(list)
}

/// `(codes->string list)`: the string of a list of code points.
fn codes_to_string(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [list] = args else {
        return Err(g.error("arity-error", args));
    };
    let mut s = String::new();
    let mut rest = *list;
    while let Some((c, next)) = g.uncons(rest) {
        let ch = match c {
            Value::Int(n) => u32::try_from(n).ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(ch) => s.push(ch),
            None => return Err(g.error("type-error", &[c])),
        }
        rest = next;
    }
    if rest != Value::Nil {
        return Err(g.error("type-error", args));
    }
    g.make_string(&s)
}

/// `(string->symbol s)`.
fn string_to_symbol(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    match args {
        [s] => match text(g, *s) {
            Some(s) => Ok(Value::Sym(g.symbol(&s)?)),
            None => Err(g.error("type-error", args)),
        },
        _ => Err(g.error("arity-error", args)),
    }
}

/// `(value->string v)`: the printed form of any value; strings print with quotes.
fn value_to_string(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    let [v] = args else {
        return Err(g.error("arity-error", args));
    };
    let mut s = String::new();
    if g.write(*v, &mut s).is_err() {
        return Err(g.error("out-of-memory", &[]));
    }
    g.make_string(&s)
}

/// `(read-string s)`: the forms of `s`, read and not evaluated.
fn read_string(g: &mut G, _: &mut Host, args: &[Value]) -> Result<Value, Error> {
    match args {
        [s] => match text(g, *s) {
            Some(src) => g.read(&src),
            None => Err(g.error("type-error", args)),
        },
        _ => Err(g.error("arity-error", args)),
    }
}

/// A `fmt::Write` into a string that stops at `ERROR_TEXT_MAX` bytes.
struct Capped(String);

impl fmt::Write for Capped {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if self.0.len() + c.len_utf8() > ERROR_TEXT_MAX {
                self.0.push_str("...");
                return Err(fmt::Error);
            }
            self.0.push(c);
        }
        Ok(())
    }
}

/// An error as its code, values and where, written out now: the values may be collected by
/// the next `eval`.
fn describe(g: &G, e: &Error) -> String {
    let mut out = Capped(String::new());
    let _ = g.write(Value::Sym(e.code), &mut out).and_then(|()| {
        e.values.iter().try_for_each(|&v| {
            fmt::Write::write_str(&mut out, " ")?;
            g.write(v, &mut out)
        })?;
        match e.at {
            Some(at) => {
                fmt::Write::write_str(&mut out, " in ")?;
                g.write(at, &mut out)
            }
            None => Ok(()),
        }
    });
    out.0
}

fn is(g: &mut G, e: &Error, code: &str) -> bool {
    g.symbol(code).is_ok_and(|c| c == e.code)
}

/// Calls the editor's function `name` with string arguments (`None` for `()`).
fn enter(g: &mut G, h: &mut Host, name: &str, args: &[Option<&str>]) -> Result<Value, Error> {
    let f = g.eval(h, name)?;
    let mut values = Vec::new();
    for a in args {
        values.push(match a {
            Some(s) => g.make_string(s)?,
            None => Value::Nil,
        });
    }
    g.call(h, f, &values)
}

const HOST: [(&str, grift::HostFn<Host>); 10] = [
    ("key", key),
    ("term-write", term_write),
    ("term-size", term_size),
    ("read-file", read_file),
    ("write-file", write_file),
    ("string->codes", string_to_codes),
    ("codes->string", codes_to_string),
    ("string->symbol", string_to_symbol),
    ("value->string", value_to_string),
    ("read-string", read_string),
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
            Err(e) if is(g, &e, "end-of-input") => return Ok(()),
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
            if !matches!(h.keys.recv(), Ok(b) if b != b'q') {
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
    let mut h = Host {
        out: Vec::new(),
        keys,
        read: 0,
    };
    let mut g: G = match Grift::new() {
        Ok(g) => g,
        Err(_) => {
            eprintln!("grift-emacs: grift failed to start");
            return ExitCode::FAILURE;
        }
    };
    g.set_interrupt_flag(&STOP);
    for (name, f) in HOST {
        if g.define(name, f).is_err() {
            eprintln!("grift-emacs: out of memory");
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = g.eval(&mut h, EDITOR) {
        eprintln!("grift-emacs: editor.grift: {}", describe(&g, &e));
        return ExitCode::FAILURE;
    }
    // An init file that fails is reported in the echo area, not fatal.
    let init = match init.map(|src| g.eval(&mut h, &src)) {
        Some(Err(e)) => format!("init: {}", describe(&g, &e)),
        _ => String::new(),
    };
    if let Err(e) = crossterm::terminal::enable_raw_mode() {
        eprintln!("grift-emacs: the terminal: {e}");
        return ExitCode::FAILURE;
    }
    // The input thread: every byte typed goes to `(key)`; C-g also interrupts whatever grift
    // is running. It ends with the input, and when the program exits.
    std::thread::spawn(move || {
        let mut b = [0u8];
        while let Ok(1) = io::stdin().lock().read(&mut b) {
            if b[0] == QUIT {
                STOP.store(true, Ordering::Relaxed);
            }
            if send.send(b[0]).is_err() {
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
