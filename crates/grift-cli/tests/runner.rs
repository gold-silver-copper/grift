//! The runner: its exit statuses, where its output goes, and each of its operations.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "a test may abort when memory is refused"
)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A scratch directory of the test's own.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `program` as a script file in `dir` with `args`, `stdin` as its input, from `dir`.
fn script(dir: &Path, program: &str, args: &[&str], stdin: &str) -> (Option<i32>, String, String) {
    let path = dir.join("script.grift");
    std::fs::write(&path, program).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg(&path)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(stdin.as_bytes()).unwrap();
    drop(input);
    let out = child.wait_with_output().unwrap();
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (out.status.code(), text(out.stdout), text(out.stderr))
}

/// Memory refused to a process's output is the same limit: the runner reads its pipe to the end,
/// so the process isn't left blocked on a full one or killed by a closed one, and waits for it to
/// end. What the process writes to stderr goes to the runner's, text or not.
#[cfg(target_os = "linux")]
#[test]
fn memory_refused_to_a_process_s_output_is_a_limit() {
    let dir = scratch("process-refused");
    let path = dir.join("big.grift");
    // Refused stdout is the limit whatever the process wrote to stderr before it.
    for before in ["", "printf '\\377' >&2; "] {
        let flood =
            format!("{before}head -c 900000000 /dev/zero; echo $? > finished; echo err >&2");
        let src = format!(
            "(catch! (finally! (run-process \"sh\" \"-c\" {flood:?}) (display \"cleanup\")) e (display \"caught\"))"
        );
        std::fs::write(&path, src).unwrap();
        let out = Command::new("bash")
            .args([
                "-c",
                "ulimit -c 0; ulimit -v 400000; exec \"$@\"",
                "process-test",
            ])
            .arg(env!("CARGO_BIN_EXE_grift"))
            .arg(&path)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            (out.status.code(), &out.stdout[..]),
            (Some(1), &b"cleanup"[..]),
            "{before}: {err}"
        );
        assert!(err.contains("out-of-memory"), "{before}: {err}");
        assert!(err.contains("err\n"), "{before}: {err}");
        assert_eq!(
            std::fs::read_to_string(dir.join("finished")).unwrap(),
            "0\n"
        );
    }
}

/// Runs the runner on `path` with `argument` under `limit` KiB of address space: its exit code,
/// `None` if a signal ended it, and what it wrote to stderr.
#[cfg(target_os = "linux")]
fn under_a_limit(dir: &Path, limit: u32, path: &Path, argument: &Path) -> (Option<i32>, String) {
    let out = Command::new("bash")
        .args(["-c", "ulimit -c 0; ulimit -v $0; exec \"$@\""])
        .arg(limit.to_string())
        .arg(env!("CARGO_BIN_EXE_grift"))
        .args([path, argument])
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A long line being read, and a path, a name, a program or an argument longer than the system
/// takes, never abort the runner, whose memory for them can be refused: each operation given
/// 30 MB ends by itself at every limit, as `out-of-memory` or as the error the system would give,
/// and with too little memory for it says `out-of-memory`.
#[cfg(target_os = "linux")]
#[test]
fn memory_refused_for_a_long_line_or_name_is_a_limit() {
    let dir = scratch("long-refused");
    let long = dir.join("long.txt");
    std::fs::write(&long, vec![b'a'; 30_000_000]).unwrap();
    let path = dir.join("long.grift");
    let on_text = [
        ("(open-file a)", "file-error"),
        ("(create-file a)", "file-error"),
        ("(read-file a)", "file-error"),
        ("(load a)", "file-error"),
        ("(environment-variable a)", "unset-variable"),
        ("(run-process \"true\" a)", "process-error"),
        ("(run-process a)", "process-error"),
    ];
    let read = "(define! a (read-file (first command-line-arguments)))";
    let programs = on_text.map(|(op, kind)| (format!("{read}{op}"), kind));
    let line = "((evaluate-in! read-line (open-file (first command-line-arguments))))";
    let line = (line.to_owned(), "out-of-memory");
    for (program, kind) in programs.iter().chain([&line]) {
        std::fs::write(&path, program).unwrap();
        for limit in [40_000, 55_000, 70_000, 85_000, 100_000, 115_000] {
            let (code, err) = under_a_limit(&dir, limit, &path, &long);
            let refused = err.contains("out-of-memory");
            // The line is read where there is room for it; the others fail whatever there is.
            let read = code == Some(0) && *kind == "out-of-memory";
            assert!(
                read || (code == Some(1) && (refused || err.contains(kind))),
                "{program} under {limit}: {code:?} {err}"
            );
            assert!(limit > 40_000 || refused, "{program} under {limit}: {err}");
        }
    }
}

/// `run-process` starts no thread: a process it runs, looking once the runner sleeps waiting for
/// its output, finds the runner has one. So no thread's start is there to abort the runner when
/// memory for it is refused.
#[cfg(target_os = "linux")]
#[test]
fn run_process_starts_no_thread() {
    let dir = scratch("no-thread");
    let look = "until grep -q '^State:.S' /proc/$PPID/status; do :; done; \
                grep '^Threads:' /proc/$PPID/status | tr -d '[:space:]'";
    let program = format!("(display (evaluate-in! output (run-process \"sh\" \"-c\" {look:?})))");
    let (status, out, err) = script(&dir, &program, &[], "");
    assert_eq!((status, out.as_str()), (Some(0), "Threads:1"), "{err}");
}

/// A runner refused memory as it starts says so: at every limit where it gives up by itself, its
/// message names `out-of-memory`.
#[cfg(target_os = "linux")]
#[test]
fn a_start_refused_memory_says_out_of_memory() {
    let dir = scratch("start-refused");
    let path = dir.join("one.grift");
    std::fs::write(&path, "(display 1)").unwrap();
    for limit in (3000..9000).step_by(8) {
        let (code, err) = under_a_limit(&dir, limit, &path, &path);
        assert!(
            code != Some(1) || err.contains("out-of-memory"),
            "under {limit}: {err}"
        );
    }
}

/// A path or an argument with a zero byte in it is the error it always was, however long: the
/// system's limit on length doesn't come before it.
#[test]
fn a_zero_byte_is_the_same_error_long_or_short() {
    let dir = scratch("zero-byte");
    let program = "(define! a (read-file (first command-line-arguments)))
        (define! said (lambda! (e) (rest (evaluate-in! values e))))
        (display (catch! (open-file a) e (said e)) (catch! (run-process \"true\" a) e (said e)))";
    let said = |name: &str, more: usize| {
        let mut text = vec![b'a'; more];
        text.extend_from_slice(b"x\0y");
        std::fs::write(dir.join(name), text).unwrap();
        script(&dir, program, &[name], "")
    };
    let (short, long) = (said("short", 0), said("long", 7 << 20));
    assert_eq!(short.0, Some(0), "{}", short.2);
    assert_eq!(short, long);
}

#[test]
fn exit_ends_the_run_with_its_status() {
    let dir = scratch("exit");
    assert_eq!(script(&dir, "(exit 7) (display 1)", &[], "").0, Some(7));
    // No catch! keeps the program going, and cleanups run.
    let (status, out, err) = script(
        &dir,
        "(catch! (finally! (exit 3) (display \"cleanup\")) e (display \"caught\"))",
        &[],
        "",
    );
    assert_eq!(
        (status, out.as_str(), err.as_str()),
        (Some(3), "cleanup", "")
    );
    // A wrong count is the arity-error a primitive gives: the operands as one list.
    let values = |call| format!("(catch! {call} e (evaluate-in! values e))");
    let counts = format!(
        "(display {} {} {})",
        values("(first 1 2)"),
        values("(exit 1 2)"),
        values("(run-process)")
    );
    assert_eq!(script(&dir, &counts, &[], "").1, "((1 2))((1 2))(())");
    // A wrong type is the type-error a primitive gives: the one value of the wrong type.
    let types = format!(
        "(display {} {} {})",
        values("(first 1)"),
        values("(read-forms 1 \"x\")"),
        values("(read-forms \"s\" 2)")
    );
    assert_eq!(script(&dir, &types, &[], "").1, "(1)(1)(2)");
    let (status, _, err) = script(&dir, "(exit 256)", &[], "");
    assert_eq!(status, Some(1));
    assert!(err.contains("range-error"));
    // A program's own error of that name is only that: it is caught, and uncaught it is reported.
    let forged = "(catch! (raise (error (quote! exit) 3)) e (display \"caught\"))";
    assert_eq!(script(&dir, forged, &[], "").1, "caught");
    let (status, _, err) = script(&dir, "(raise (error (quote! exit) 3))", &[], "");
    assert_eq!(status, Some(1));
    assert!(err.contains("exit at "), "{err}");
    assert!(err.contains("script.grift:1 3 in (raise"), "{err}");
}

#[test]
fn bytes_that_are_not_text_go_in_and_out() {
    let dir = scratch("bytes");
    // Nine full reads of 65536 and seven bytes more, every byte value, and not UTF-8.
    let input: Vec<u8> = (0..9 * 65536 + 7_u32)
        .map(|at| ((at * 73 + at / 251) % 256) as u8)
        .collect();
    assert!(std::str::from_utf8(&input).is_err());
    let path = dir.join("bytes.grift");
    // Standard input read in counted pieces until an empty one, joined, to a file whole, and its
    // last 100 bytes to standard output: tail -c 100.
    let program = r#"
        (define! (copy) command-line-arguments)
        (define! read (evaluate-in! read-bytes standard-input))
        (define! put (evaluate-in! write standard-output))
        (define! whole
          (lambda! (chunks)
            (let! ((chunk (read 65536)))
              (if! (same? chunk #bytes()) (apply join (reverse chunks)) (whole (pair chunk chunks))))))
        (define! all (whole ()))
        (put (slice all (subtract (byte-length all) 100) (byte-length all)))
        (define! out (create-file copy))
        ((evaluate-in! write out) all)
        ((evaluate-in! close out))
        (define! in (open-file copy))
        (display-error (byte-length ((evaluate-in! read-bytes in) 1000)) " " ((evaluate-in! read-bytes in) 0) " "
                       (catch! (put (list 1)) e (evaluate-in! kind e)))
    "#;
    std::fs::write(&path, program).unwrap();
    let copy = dir.join("copy");
    let mut child = Command::new(env!("CARGO_BIN_EXE_grift"))
        .args([path.to_str().unwrap(), copy.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A runner that fails before reading closes its end; what it said is the assertion below.
    let _ = child.stdin.take().unwrap().write_all(&input);
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "1000 #bytes() type-error"
    );
    assert_eq!(out.stdout, input[input.len() - 100..]);
    assert_eq!(std::fs::read(&copy).unwrap(), input);
    // A file that isn't text can't be read as text, but its bytes can be.
    let program = format!(
        "(display (catch! (read-file {copy:?}) e (evaluate-in! kind e)) (catch! (load {copy:?}) e (evaluate-in! kind e)))"
    );
    assert_eq!(
        script(&dir, &program, &[], "").1,
        "encoding-errorencoding-error"
    );
}

/// The write end of a pipe whose read end is gone.
fn closed_pipe() -> std::io::PipeWriter {
    std::io::pipe().unwrap().1
}

#[test]
fn a_closed_stderr_does_not_panic_the_runner() {
    for script in ["--help", "no/such.grift"] {
        let status = Command::new(env!("CARGO_BIN_EXE_grift"))
            .arg(script)
            .stdin(Stdio::null())
            .stderr(closed_pipe())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(1));
    }
}

/// Types `input` at grift on a terminal of its own, once it has asked, and gives the exit status
/// and everything the terminal showed after that first prompt.
#[cfg(target_os = "linux")]
fn at_a_terminal(input: &str) -> (Option<i32>, String) {
    use std::io::Read;
    let mut child = Command::new("script")
        .args(["-qec", "stty -echo; exec \"$GRIFT\"", "/dev/null"])
        .env("GRIFT", env!("CARGO_BIN_EXE_grift"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut asked = [0; 2];
    stdout.read_exact(&mut asked).unwrap();
    assert_eq!(&asked, b"> ");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let mut shown = String::new();
    stdout.read_to_string(&mut shown).unwrap();
    (child.wait().unwrap().code(), shown.replace("\r\n", "\n"))
}

/// A session's lines are numbered as one file's are, from its first line typed, so `stdin:N` in a
/// trace is the Nth line of the session whichever input it was in.
#[cfg(target_os = "linux")]
#[test]
fn a_session_numbers_its_lines_from_its_start() {
    let (status, shown) =
        at_a_terminal("1\n(define! f\n  (lambda! () (first 1)))\n\n(add 1\n (f))\n\x04");
    assert_eq!(status, Some(0), "{shown}");
    assert!(
        shown.contains(
            "stdin: type-error at stdin:3 1 in (first 1)\n  from (add 1 (f)) at stdin:5\n"
        ),
        "{shown}"
    );
    let (_, shown) = at_a_terminal("1\n2\n)\n\x04");
    assert!(
        shown.contains("stdin: parse-error unmatched-parenthesis 3 1 stdin\n"),
        "{shown}"
    );
    // A string open at a line's end goes on to the next line, as a list does.
    let (_, shown) = at_a_terminal("\"a\nb\"\n(first 1)\n\x04");
    assert!(
        shown.contains("stdin: type-error at stdin:3 1 in (first 1)\n")
            && !shown.contains("parse-error"),
        "{shown}"
    );
    // So is input ending inside a form.
    let (_, shown) = at_a_terminal("1\n(add 1\n\x04");
    assert!(
        shown.contains("stdin: parse-error unclosed-list 2 1 stdin\n"),
        "{shown}"
    );
}

/// A report that fits its budget is written whole and says so, at exactly its size: a name is cut
/// only to keep the place in a report that doesn't fit, and the kind comes first. Through the
/// runner, a long kind in a report under 4 KiB is whole, with no line saying it was cut.
#[test]
fn a_report_that_fits_its_budget_is_whole() {
    use grift::{Grift, ReportStatus, TextBuffer};
    let mut g: Grift = Grift::new().unwrap();
    let e = g.evaluate(&mut (), "(first 1)").unwrap_err();
    let report = |bytes| {
        let mut text = TextBuffer::new();
        let status = g.describe(&e, &mut text, bytes).unwrap();
        (status, text.as_str().to_owned())
    };
    let whole = "type-error 1 in (first 1)";
    for bytes in [whole.len(), whole.len() + 1, 4096] {
        assert_eq!(report(bytes), (ReportStatus::Complete, whole.to_owned()));
    }
    for bytes in [whole.len() - 1, 12] {
        assert_eq!(
            report(bytes),
            (ReportStatus::Truncated, whole[..bytes].to_owned())
        );
    }
    let dir = scratch("long-kind");
    let kind = "k".repeat(1100);
    let program = format!("(raise (error (string-to-symbol \"{kind}\") 1))");
    let (status, _, err) = script(&dir, &program, &[], "");
    assert_eq!(status, Some(1));
    assert!(err.contains(&format!("{kind} at ")), "{err}");
    assert_eq!(err.lines().count(), 1, "{err}");
}
