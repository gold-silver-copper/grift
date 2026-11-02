//! The runner: its exit statuses, where its output goes, and each of its operations.

use std::io::Write;
use std::path::{Path, PathBuf};
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

/// A scratch directory of the test's own.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `program` as a script file with `args`, `stdin` as its input.
fn script(dir: &Path, program: &str, args: &[&str], stdin: &str) -> (Option<i32>, String, String) {
    let path = dir.join("script.grift");
    std::fs::write(&path, program).unwrap();
    let path = path.to_str().unwrap();
    let out = run(&[&[path], args].concat(), stdin, Stdio::piped());
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (out.status.code(), text(out.stdout), text(out.stderr))
}

#[test]
fn statuses() {
    let ok = run(
        &[],
        "(define! f (lambda! (x) (multiply x x))) (display (f 7)) (newline)",
        Stdio::piped(),
    );
    assert_eq!((ok.status.code(), &ok.stdout[..]), (Some(0), &b"49\n"[..]));

    let failed = run(&[], "(first 5)", Stdio::piped());
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("type-error"));

    // A program can't pass its own error off as a closed stdout.
    let spoof = run(
        &[],
        "(raise (error (quote! stdout-closed) 1))",
        Stdio::piped(),
    );
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

#[test]
fn standard_input_is_data_for_a_script() {
    let dir = scratch("standard-input");
    // read-line gives each line with its newline, then "" at the end.
    let lines = "(define! read-line (evaluate-in! read-line standard-input))
                 (display (list (read-line) (read-line) (read-line)))";
    let (status, out, _) = script(&dir, lines, &[], "a\nb");
    assert_eq!((status, out.as_str()), (Some(0), "(\"a\\n\" \"b\" \"\")"));
    let all = "(display ((evaluate-in! read-all standard-input)))";
    let (_, out, _) = script(&dir, all, &[], "x\ny\n");
    assert_eq!(out, "x\ny\n");
    // A program read from stdin can't read it again.
    let out = run(
        &[],
        "((evaluate-in! read-line standard-input))",
        Stdio::piped(),
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("closed \"standard-input\""));
}

#[test]
fn files_are_contexts_of_operations() {
    let dir = scratch("files");
    let path = dir.join("out.txt");
    let path = path.to_str().unwrap();
    let program = r#"
        (define! (path) command-line-arguments)
        (define! out (create-file path))
        (display out " " (evaluate-in! path out)) (newline)
        ((evaluate-in! write out) "one\ntwo\n")
        ((evaluate-in! close out))
        (define! in (open-file path))
        (display in) (newline)
        (display (list ((evaluate-in! read-line in)) ((evaluate-in! read-all in)) ((evaluate-in! read-line in))))
        (newline)
        ((evaluate-in! close in))
        ; Using a file after it is closed is the error closed.
        (display (catch! ((evaluate-in! read-line in)) e (list (evaluate-in! kind e) (evaluate-in! values e))))
        (newline)
        (display (catch! ((evaluate-in! close out)) e (evaluate-in! kind e)))
        (newline)
        (display (catch! (open-file "no/such/file") e (evaluate-in! kind e)))
        (newline)
        ; The host functions behind the operations are bound nowhere a program can reach.
        (display (binds!? (current-context) open-host read-line-host write-host close-host run-host))
    "#;
    let (status, out, err) = script(&dir, program, &[path], "");
    assert_eq!(status, Some(0), "{err}");
    let want = format!(
        "#[context close path write] {path}\n#[context close path read-all read-line]\n\
         (\"one\\n\" \"two\\n\" \"\")\n(closed (\"{path}\"))\nclosed\nfile-error\n#false"
    );
    assert_eq!(out, want);
    assert_eq!(std::fs::read(path).unwrap(), b"one\ntwo\n");
}

#[test]
fn a_program_cannot_reach_the_runner_s_host_functions() {
    let dir = scratch("boundary");
    // Redefining the library names the file operations use changes nothing for them: they look
    // names up in a private context, never in the program's.
    let program = r#"
        (define! top (current-context))
        (define! leaked #inert)
        (define! spy (vau! #ignore caller (sequence! (define-in! top leaked caller) #inert)))
        (define! lambda! spy) (define! quote! spy) (define! list spy) (define! context "scratch")
        (define! in (open-file "/dev/null"))
        (display (same? leaked #inert) " " in " " ((evaluate-in! read-all in)))
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(out, "#true #[context close path read-all read-line] ");
}

#[test]
fn command_line_arguments_and_environment_variables() {
    let dir = scratch("arguments");
    let program = "(display command-line-arguments (environment-variable \"GRIFT_TEST\"))
                   (display (catch! (environment-variable \"GRIFT_UNSET\") e (evaluate-in! kind e)))";
    std::fs::write(dir.join("a.grift"), program).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_grift"))
        .args([dir.join("a.grift").to_str().unwrap(), "-x", "two words"])
        .env("GRIFT_TEST", "set")
        .env_remove("GRIFT_UNSET")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "(\"-x\" \"two words\")setunset-variable"
    );
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
    let (status, _, err) = script(&dir, "(exit 256)", &[], "");
    assert_eq!(status, Some(1));
    assert!(err.contains("range-error"));
}

#[test]
fn run_process_gives_a_record() {
    let dir = scratch("process");
    let program = r#"
        (define! ran (run-process "sh" "-c" "echo out; echo err >&2; exit 4"))
        (display ran (list (evaluate-in! status ran) (evaluate-in! output ran) (evaluate-in! error-output ran)))
        (display (catch! (run-process "no-such-program-here") e (evaluate-in! kind e)))
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(
        out,
        "#[context error-output output status](4 \"out\\n\" \"err\\n\")process-error"
    );
}

#[test]
fn a_script_counts_its_input() {
    // A whole script, as a Unix tool runs it: a #! line, an argument, standard input, a file
    // written and read back with another program, and an exit status.
    let dir = scratch("count-words");
    let report = dir.join("report.txt");
    let report = report.to_str().unwrap();
    let program = include_str!("count-words.grift");
    let (status, out, err) = script(&dir, program, &[report], "one two  three\n\nfour\tfive\n");
    assert_eq!(status, Some(0), "{err}");
    assert!(out.starts_with("report: 3 5\n"), "{out}");
    assert_eq!(std::fs::read_to_string(report).unwrap(), "3 5\n");
    assert_eq!(script(&dir, program, &[report], "").0, Some(3));
}
