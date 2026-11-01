//! The runner's exit statuses and where its output goes.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &str, stdout: Stdio) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_grift"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn statuses() {
    let ok = run(
        &[],
        "($define! f ($lambda (x) (* x x))) (display (f 7)) (newline)",
        Stdio::piped(),
    );
    assert_eq!((ok.status.code(), &ok.stdout[..]), (Some(0), &b"49\n"[..]));

    let failed = run(&[], "(car 5)", Stdio::piped());
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("type-error"));

    // A program can't pass its own error off as a closed stdout.
    let spoof = run(&[], "(error ($quote stdout-closed) 1)", Stdio::piped());
    assert_eq!(spoof.status.code(), Some(1));

    assert_eq!(run(&["--help"], "", Stdio::piped()).status.code(), Some(2));
    assert_eq!(
        run(&["no/such/file.grift"], "", Stdio::piped())
            .status
            .code(),
        Some(2)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_failed_write_is_an_error() {
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let out = run(&[], "(display \"data\")", Stdio::from(full));
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("output-error"));
}
