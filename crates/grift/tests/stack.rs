#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

#[test]
fn named_evaluations_through_host_callbacks_fit_a_small_stack() {
    // Embedders normally develop without optimization. The workspace test profile optimizes
    // grift, which otherwise hides the stack cost of temporaries in long-lived machine frames.
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("small-stack-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let library = scratch.join("libgrift.rlib");
    let host = scratch.join("host");
    let rustc = env!("GRIFT_TEST_RUSTC");
    let built = Command::new(rustc)
        .args(["--edition=2024", "--crate-name=grift", "--crate-type=rlib"])
        .args(["-Copt-level=0", "-Ccodegen-units=1", "-Funsafe-code"])
        .arg(source.join("src/lib.rs"))
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let built = Command::new(rustc)
        .args([
            "--edition=2024",
            "-Copt-level=0",
            "-Ccodegen-units=1",
            "-Funsafe-code",
        ])
        .arg(source.join("tests/host/main.rs"))
        .arg("--extern")
        .arg(format!("grift={}", library.display()))
        .arg("-o")
        .arg(&host)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    // A stack overflow aborts the process, so isolate the host from the test harness.
    let ran = Command::new(&host).output().unwrap();
    std::fs::remove_dir_all(&scratch).unwrap();
    assert!(
        ran.status.success(),
        "{}: {}",
        ran.status,
        String::from_utf8_lossy(&ran.stderr)
    );
}
