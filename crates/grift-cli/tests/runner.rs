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
        "(define! f (lambda! (x) (multiply x x))) (display (f 7) \"\\n\")",
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
        (display out " " (evaluate-in! path out)) (display "\n")
        ((evaluate-in! write out) "one\ntwo\n")
        ((evaluate-in! close out))
        (define! in (open-file path))
        (display in) (display "\n")
        (display (list ((evaluate-in! read-line in)) ((evaluate-in! read-all in)) ((evaluate-in! read-line in))))
        (display "\n")
        ((evaluate-in! close in))
        ; Using a file after it is closed is the error closed.
        (display (catch! ((evaluate-in! read-line in)) e (list (evaluate-in! kind e) (evaluate-in! values e))))
        (display "\n")
        (display (catch! ((evaluate-in! close out)) e (evaluate-in! kind e)))
        (display "\n")
        (display (catch! (open-file "no/such/file") e (evaluate-in! kind e)))
        (display "\n")
        ; The host functions behind the operations are bound nowhere a program can reach.
        (display (any? (lambda! (name) (binds? (current-context) name)) (quote! (open-host read-line-host write-host close-host run-host))))
    "#;
    let (status, out, err) = script(&dir, program, &[path], "");
    assert_eq!(status, Some(0), "{err}");
    let want = format!(
        "#[context close path write write-bytes] {path}\n\
         #[context close path read-all read-bytes read-line]\n\
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
        (define! spy (vau! #ignore caller (sequence! (bind top (quote! leaked) caller) #inert)))
        (define! lambda! spy) (define! quote! spy) (define! list spy) (define! context "scratch")
        (define! in (open-file "/dev/null"))
        (display (same? leaked #inert) " " in " " ((evaluate-in! read-all in)))
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(
        out,
        "#true #[context close path read-all read-bytes read-line] "
    );
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
fn standard_error_is_written_to_as_a_file_is() {
    let dir = scratch("standard-error");
    // The text goes to stderr, nothing to stdout, and the status can then be any.
    let say = "((evaluate-in! write standard-error) \"no rule\\n\") (display \"data\") (exit 2)";
    let (status, out, err) = script(&dir, say, &[], "");
    assert_eq!(
        (status, out.as_str(), err.as_str()),
        (Some(2), "data", "no rule\n")
    );
    // Also for a program read from stdin, whose standard input alone is closed.
    let out = run(&[], say, Stdio::piped());
    assert_eq!(out.stderr, b"no rule\n");
    let (status, _, err) = script(&dir, "((evaluate-in! write standard-error) 1)", &[], "");
    assert_eq!(status, Some(1));
    assert!(err.contains("type-error"));
    // Writes add to what is there: it is the descriptor, not a path to empty.
    let log = dir.join("log");
    let held = std::fs::File::create(&log).unwrap();
    writeln!(&held, "before").unwrap();
    let path = dir.join("say.grift");
    std::fs::write(&path, say).unwrap();
    Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg(path)
        .stdout(Stdio::null())
        .stderr(held)
        .status()
        .unwrap();
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "before\nno rule\n");
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

#[test]
fn load_runs_a_file_s_forms_where_it_is_called() {
    let dir = scratch("load");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    // A relative path is relative to the file being loaded: lib/a.grift loads lib/b.grift.
    std::fs::write(
        dir.join("lib/a.grift"),
        "(define! a 1)\n(load \"b.grift\")\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("lib/b.grift"),
        "(define! loads (add loads 1))\n(define! b (lambda! (x) (add x a)))\n\n(define! c (first 2))\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("lib/bad.grift"),
        "(display \"ran\")\n(define! f\n  (lambda! (x)\n",
    )
    .unwrap();
    let program = r#"
        (define! loads 0)
        (define! write-trace (lambda! (place) (display (value-to-string place) " ")))
        (catch-kind! (load "lib/a.grift") type-error e (write-trace (last (evaluate-in! trace e))))
        (display (b 1) " " loads " ")
        ; Each load runs the file again; in a child context, its definitions are the child's.
        (define! child (context (current-context)))
        (catch! (evaluate (quote! (load "lib/b.grift")) child) #ignore #inert)
        (display loads " " (evaluate (quote! loads) child) " ")
        ; A file that doesn't read runs nothing, and says where the form opened.
        (catch! (load "lib/bad.grift") e (sequence! (display (evaluate-in! values e)) (write-trace (second (evaluate-in! trace e)))))
        (display (string-ends-with? script-path "script.grift"))
        (load "lib/b.grift")
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    let lib = dir.join("lib");
    let lib = lib.to_str().unwrap();
    assert_eq!(status, Some(1));
    assert_eq!(
        out,
        format!("\"{lib}/a.grift:2\" 2 1 1 2 (unclosed-list 3 3)\"{lib}/bad.grift\" #true")
    );
    // The trace says the file and the line of the form, between its combinations and the load.
    assert!(
        err.contains(&format!(
            "in (first 2)\n  from (define! c (first 2))\n  from \"{lib}/b.grift:4\""
        )),
        "{err}"
    );
    assert!(err.ends_with("from (load \"lib/b.grift\")\n"), "{err}");
}

#[test]
fn bytes_that_are_not_text_go_in_and_out() {
    let dir = scratch("bytes");
    let every: Vec<u8> = (0..=255).collect();
    let path = dir.join("bytes.grift");
    // Standard input to standard output in pieces of 100 bytes, and to a file in one.
    let program = r#"
        (define! (copy) command-line-arguments)
        (define! read (evaluate-in! read-bytes standard-input))
        (define! put (evaluate-in! write-bytes standard-output))
        (define! pass (lambda! (all) (let! ((bytes (read 100))) (if! (empty-list? bytes) all (sequence! (put bytes) (pass (append all bytes)))))))
        (define! out (create-file copy))
        ((evaluate-in! write-bytes out) (pass ()))
        ((evaluate-in! close out))
        (display-error (length ((evaluate-in! read-bytes (open-file copy)) 1000)) " "
                       (catch! (put (list 256)) e (evaluate-in! kind e)))
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
    child.stdin.take().unwrap().write_all(&every).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.stdout, every);
    assert_eq!(std::fs::read(&copy).unwrap(), every);
    assert_eq!(out.stderr, b"256 range-error");
    // A file that isn't text can't be read as text, but its bytes can be.
    let program = format!(
        "(display (catch! (read-file {copy:?}) e (evaluate-in! kind e)) (catch! (load {copy:?}) e (evaluate-in! kind e)))"
    );
    assert_eq!(
        script(&dir, &program, &[], "").1,
        "encoding-errorencoding-error"
    );
}

#[test]
fn read_file_display_error_and_the_clocks() {
    let dir = scratch("read-file");
    let path = dir.join("text");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    let path = path.to_str().unwrap();
    let program = r#"
        (define! (path) command-line-arguments)
        (display (read-file path) (catch! (read-file "no/such") e (evaluate-in! kind e)))
        (display-error "wc: " path ": " 2 " " (list "a" 1.5) "\n")
        (define! a (elapsed-nanoseconds))
        (define! b (elapsed-nanoseconds))
        (display " " (not? (less-than? b a)) " " (wall-clock-nanoseconds))
    "#;
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let (status, out, err) = script(&dir, program, &[path], "");
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(err, format!("wc: {path}: 2 (\"a\" 1.5)\n"));
    let (text, wall) = out.rsplit_once(' ').unwrap();
    assert_eq!(text, "one\ntwo\nfile-error #true");
    let wall: u128 = wall.parse().unwrap();
    assert!(
        wall >= before && wall - before < 60_000_000_000,
        "{wall} {before}"
    );
}

#[test]
fn a_parse_error_says_where_the_form_opened() {
    let dir = scratch("parse-error");
    let (status, _, err) = script(
        &dir,
        "(display 1)\n(define! f\n  (lambda! (x)\n    (add x 1))\n",
        &[],
        "",
    );
    assert_eq!(status, Some(1));
    assert!(
        err.ends_with("script.grift: parse-error unclosed-list 2 1\n"),
        "{err}"
    );
}
