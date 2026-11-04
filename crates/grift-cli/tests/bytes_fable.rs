//! The runner's `read-bytes` and `write-bytes` with bytes values: `tail -c 100` of a large input
//! that isn't text gives exactly its last 100 bytes. A property, never a timing.
//!
//! This compiles on the base and fails at run time: the base has no bytes value, so the program's
//! `#bytes()` is `parse-error unknown-constant`.

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn tail_c_100_of_a_large_input_that_is_not_text_is_exact() {
    // About 589 KB, every byte value, and not UTF-8 from its first bytes on.
    let input: Vec<u8> = (0..589_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    assert!(std::str::from_utf8(&input).is_err());
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tail-bytes-fable");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Standard input read in counted pieces until an empty one, joined, and the last 100 bytes
    // written: the shape of tail -c, on bytes values.
    let program = r#"
        (define! read (evaluate-in! read-bytes standard-input))
        (define! put (evaluate-in! write-bytes standard-output))
        (define! read-whole
          (lambda! (chunks)
            (let! ((chunk (read 65536)))
              (if! (same? chunk #bytes())
                (apply bytes-append (reverse chunks))
                (read-whole (pair chunk chunks))))))
        (define! all (read-whole ()))
        (define! n (bytes-length all))
        (define! count (if! (less-than? n 100) n 100))
        (put (bytes-slice all (subtract n count) n))
    "#;
    let path = dir.join("tail.grift");
    std::fs::write(&path, program).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A runner that fails before reading closes its end; what it said is the assertion below.
    let _ = child.stdin.take().unwrap().write_all(&input);
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "tail failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, input[input.len() - 100..]);
}
