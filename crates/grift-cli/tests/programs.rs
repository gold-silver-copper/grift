//! The corpus: every program in `programs/` passes its own `test.sh` on this runner, four at a time.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

#[test]
fn every_program_passes_its_own_test() {
    let programs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../programs");
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs");
    let mut names: Vec<_> = std::fs::read_dir(&programs)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    let queue = Mutex::new(names.into_iter());
    let failed = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                loop {
                    let next = queue.lock().unwrap().next();
                    let Some(name) = next else { break };
                    let tmp = scratch.join(&name);
                    let _ = std::fs::remove_dir_all(&tmp);
                    std::fs::create_dir_all(&tmp).unwrap();
                    let out = Command::new("bash")
                        .arg(programs.join(&name).join("test.sh"))
                        .env("GRIFT", env!("CARGO_BIN_EXE_grift"))
                        .env("TMPDIR", &tmp)
                        .output()
                        .unwrap();
                    if !out.status.success() {
                        let text = String::from_utf8_lossy(&out.stdout);
                        let error = String::from_utf8_lossy(&out.stderr);
                        failed.lock().unwrap().push(format!("{name:?}:\n{text}\n{error}"));
                    }
                }
            });
        }
    });
    let failed = failed.into_inner().unwrap();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
