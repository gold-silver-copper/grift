//! A closed stderr, in a test binary of its own: a child another test's thread is starting holds a
//! copy of every descriptor until it runs its program, and a copy of stderr's read end lets the
//! write succeed. Cargo runs test binaries one at a time, so nothing else starts a child here.

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn a_closed_stderr_is_the_same_as_any_closed_pipe() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_grift"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stderr.take());
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"(display-error \"lost\") (display \"after\")")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!((out.status.code(), &out.stdout[..]), (Some(141), &b""[..]));
}
