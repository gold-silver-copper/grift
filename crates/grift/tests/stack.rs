#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

// Include the standalone source for fmt, type checking and clippy, without compiling another
// optimized interpreter. The test below builds and runs it unoptimized in its own process.
#[path = "fixtures/host.rs"]
#[allow(dead_code)]
mod host;

#[test]
fn named_evaluations_through_host_callbacks_fit_a_small_stack() {
    // Embedders normally develop without optimization. The workspace test profile optimizes
    // grift, which otherwise hides the stack cost of temporaries in long-lived machine frames.
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("small-stack-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let manifest = format!(
        "[package]\nname = \"host\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
         [[bin]]\nname = \"host\"\npath = {:?}\n\
         [dependencies]\ngrift = {{ path = {:?} }}\n\
         [profile.dev]\ncodegen-units = 1\n\
         [workspace]\n",
        source.join("tests/fixtures/host.rs"),
        source,
    );
    std::fs::write(scratch.join("Cargo.toml"), manifest).unwrap();
    // A stack overflow aborts the process, so isolate the host from the test harness.
    let ran = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--offline"])
        .current_dir(&scratch)
        .env("CARGO_TARGET_DIR", scratch.join("target"))
        .output()
        .unwrap();
    std::fs::remove_dir_all(&scratch).unwrap();
    assert!(
        ran.status.success(),
        "{}: {}",
        ran.status,
        String::from_utf8_lossy(&ran.stderr)
    );
}
