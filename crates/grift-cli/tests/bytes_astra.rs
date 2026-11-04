use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn tail_keeps_the_last_100_bytes_of_large_binary_input() {
    // Nine full reads and seven bytes: the last 100 span the final two nonempty reads.
    let input: Vec<u8> = (0..(9 * 65_536 + 7_u32))
        .map(|at| ((at * 73 + at / 251) % 256) as u8)
        .collect();
    assert!(std::str::from_utf8(&input).is_err());
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("bytes-astra-tail-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("tail.grift");
    let data = dir.join("input.bin");
    fs::write(&data, &input).unwrap();
    fs::write(
        &script,
        r#"
        (define! read (evaluate-in! read-bytes standard-input))
        (define! put (evaluate-in! write-bytes standard-output))
        (define! tail
          (lambda! (last)
            (define! chunk (read 65536))
            (if! (same? (type-of chunk) (quote! bytes))
              (if! (same? (bytes-length chunk) 0)
                (put last)
                (let! ((joined (bytes-append last chunk)))
                  (tail (bytes-slice joined
                          (maximum 0 (subtract (bytes-length joined) 100))
                          (bytes-length joined)))))
              (raise (error (quote! read-bytes-returned-non-bytes) (type-of chunk))))))
        (tail (read 0))
        "#,
    )
    .unwrap();
    // A file as stdin also lets an early interpreter error reach the assertion without a
    // failed write to a closed input pipe hiding it.
    let result = Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg(&script)
        .stdin(Stdio::from(File::open(&data).unwrap()))
        .output();
    fs::remove_dir_all(&dir).unwrap();
    let output = result.unwrap();
    assert!(
        output.status.success(),
        "tail failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, input[input.len() - 100..]);
}
