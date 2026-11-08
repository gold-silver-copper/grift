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

    // A program can't pass its own error off as a closed pipe.
    let spoof = run(
        &[],
        "(raise (error (quote! broken-pipe) 1))",
        Stdio::piped(),
    );
    assert_eq!(spoof.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&spoof.stderr).contains("broken-pipe"));

    let unreadable = run(&["no/such/file.grift"], "", Stdio::piped());
    assert_eq!(unreadable.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unreadable.stderr).contains("file-error"));
}

#[test]
fn the_first_argument_is_the_script_whatever_its_spelling() {
    let dir = scratch("flag-spelling");
    std::fs::write(dir.join("-x.grift"), "(display 42)").unwrap();
    let ran = Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg("-x.grift")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!((ran.status.code(), &ran.stdout[..]), (Some(0), &b"42"[..]));
    // One that can't be read says why, then how to run grift.
    let help = run(&["--help"], "", Stdio::piped());
    let said = String::from_utf8(help.stderr).unwrap();
    assert!(said.starts_with("grift: file-error \"--help\" "), "{said}");
    assert!(said.contains("\nusage: grift"), "{said}");
    assert_eq!(help.status.code(), Some(1));
}

#[cfg(target_os = "linux")]
#[test]
fn a_failed_write_is_an_error() {
    // One failure, one kind, whichever word wrote.
    for program in [
        "(display \"data\")",
        "((evaluate-in! write standard-output) \"data\")",
    ] {
        let full = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap();
        let out = run(&[], program, Stdio::from(full));
        assert_eq!(out.status.code(), Some(1));
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(said.contains("file-error \"standard-output\""), "{said}");
    }
}

#[test]
fn a_standard_stream_is_a_file_like_any_other() {
    let dir = scratch("standard-streams");
    let program = r#"
        (display (equal? (names standard-input) (names (open-file "/dev/null")))
                 (equal? (names standard-output) (names (create-file "/dev/null")))
                 (evaluate-in! path standard-error))
        ((evaluate-in! close standard-output))
        (display-error (catch! (display "lost") e (list (evaluate-in! kind e) (evaluate-in! values e))))
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(out, "#true#truestandard-error");
    assert_eq!(err, "(closed (\"standard-output\"))");
}

/// Memory refused to a read is the limit `out-of-memory`: its cleanups run and no `catch!` keeps it.
#[cfg(target_os = "linux")]
#[test]
fn memory_refused_to_a_read_is_a_limit() {
    let dir = scratch("read-refused");
    let path = dir.join("big.grift");
    for read in [
        "((evaluate-in! read-bytes (open-file \"/dev/zero\")) 900000000)",
        "((evaluate-in! read-all (open-file \"/dev/zero\")))",
        "(read-file \"/dev/zero\")",
    ] {
        let src =
            format!("(catch! (finally! {read} (display \"cleanup\")) e (display \"caught\"))");
        std::fs::write(&path, src).unwrap();
        let out = Command::new("bash")
            .args([
                "-c",
                "ulimit -c 0; ulimit -v 400000; exec \"$@\"",
                "read-test",
            ])
            .arg(env!("CARGO_BIN_EXE_grift"))
            .arg(&path)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(
            (out.status.code(), &out.stdout[..]),
            (Some(1), &b"cleanup"[..])
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("out-of-memory"));
    }
}

/// Memory refused to a process's output is the same limit: the runner reads both of its pipes to
/// their ends, so the process isn't left blocked on a full one or killed by a closed one, and waits
/// for it to end.
#[cfg(target_os = "linux")]
#[test]
fn memory_refused_to_a_process_s_output_is_a_limit() {
    let dir = scratch("process-refused");
    let path = dir.join("big.grift");
    // Refused stdout is the limit even when stderr isn't text, which alone is catchable.
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
        assert_eq!(
            std::fs::read_to_string(dir.join("finished")).unwrap(),
            "0\n"
        );
    }
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
    // A program read from stdin has read it to its end, so more reads find nothing.
    let out = run(
        &[],
        "(display (list ((evaluate-in! read-all standard-input)) ((evaluate-in! read-line standard-input))))",
        Stdio::piped(),
    );
    assert_eq!(
        (out.status.code(), &out.stdout[..]),
        (Some(0), &b"(\"\" \"\")"[..])
    );
}

#[test]
fn script_path_is_always_bound() {
    let out = run(&[], "(display (list script-path))", Stdio::piped());
    assert_eq!(out.stdout, b"(\"\")");
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
        "#[context close path write] {path}\n\
         #[context close path read-all read-bytes read-line]\n\
         (\"one\\n\" \"two\\n\" \"\")\n(closed (\"{path}\"))\nclosed\nfile-error\n#false"
    );
    assert_eq!(out, want);
    assert_eq!(std::fs::read(path).unwrap(), b"one\ntwo\n");
}

/// Each read or load keeps nothing, one that fails included: what 120,000 of them would keep, a 4
/// KB path each, doesn't fit the 400 MB of address space given here, nor 400 files left open the 64
/// descriptors.
#[cfg(target_os = "linux")]
#[test]
fn reading_and_loading_again_and_again_keep_nothing() {
    let dir = scratch("repeated-reads");
    let path = format!("/dev/{}null", "./".repeat(2000));
    let bad = dir.join("not-text");
    std::fs::write(&bad, b"\xff").unwrap();
    let file = dir.join("reads.grift");
    let program = format!(
        "(define! path \"{path}\")
         (define! again
           (lambda! (n)
             (when! (less-than? 0 n) (read-file path) (load path) (again (subtract n 1)))))
         (again 60000)
         (define! fail
           (lambda! (n)
             (when! (less-than? 0 n)
               (catch-kind! (read-file {bad:?}) encoding-error e #inert)
               (catch-kind! (load {bad:?}) encoding-error e #inert)
               (fail (subtract n 1)))))
         (fail 200)
         (display \"done\")"
    );
    std::fs::write(&file, program).unwrap();
    let out = Command::new("prlimit")
        .args(["--as=400000000", "--nofile=64", env!("CARGO_BIN_EXE_grift")])
        .arg(&file)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{}", &err[..err.len().min(300)]);
    assert_eq!(out.stdout, b"done");
}

/// A file a program closed stays closed whatever is read or opened after it.
#[test]
fn a_closed_file_stays_closed_past_later_reads_and_opens() {
    let dir = scratch("closed-stays");
    let (a, b) = (dir.join("a"), dir.join("b"));
    std::fs::write(&a, "from a").unwrap();
    std::fs::write(&b, "from b").unwrap();
    let program = r#"
        (define! (a b) command-line-arguments)
        (define! one (open-file a))
        ((evaluate-in! close one))
        (define! read (read-file b))
        (define! two (open-file b))
        (display (list read
                       ((evaluate-in! read-all two))
                       (catch! ((evaluate-in! read-all one)) e (evaluate-in! kind e))))
    "#;
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());
    let (status, out, err) = script(&dir, program, &[a, b], "");
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(out, "(\"from b\" \"from b\" closed)");
}

/// The usage is for a script that isn't there. Text that can't be read says only why.
#[test]
fn only_a_script_that_is_not_there_prints_the_usage() {
    let dir = scratch("usage");
    let file = dir.join("not-text.grift");
    std::fs::write(&file, b"\xff").unwrap();
    let as_script = Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg(&file)
        .output()
        .unwrap();
    let as_stdin = Command::new(env!("CARGO_BIN_EXE_grift"))
        .stdin(std::fs::File::open(&file).unwrap())
        .output()
        .unwrap();
    for out in [as_script, as_stdin] {
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{err}");
        assert!(err.contains("encoding-error"), "{err}");
        assert!(!err.contains("usage:"), "{err}");
    }
    let missing = Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg(dir.join("nothing-here"))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&missing.stderr).contains("usage:"));
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

/// The names the README's table of operations gives a program, sorted.
fn documented_operations() -> Vec<String> {
    let readme = include_str!("../README.md");
    let (_, table) = readme.split_once("| Operation | Meaning |").unwrap();
    let rows = table
        .lines()
        .skip(2)
        .take_while(|line| line.starts_with('|'));
    let mut names: Vec<String> = rows
        .flat_map(|row| row.split('|').nth(1).unwrap().split('`').skip(1).step_by(2))
        .map(|code| {
            code.trim_start_matches('(')
                .split([' ', ')'])
                .next()
                .unwrap()
                .to_string()
        })
        .collect();
    names.sort();
    names
}

/// A program, as a script or at a terminal, gets exactly the operations the README lists, and none
/// of the names `runner.grift` uses for itself: no host function behind them, no helper.
#[test]
fn a_program_reaches_exactly_the_documented_operations() {
    let runner = concat!(env!("CARGO_MANIFEST_DIR"), "/src/runner.grift");
    let probe = format!(
        "((lambda! (here top-names)
            (define! symbols
              (lambda! (form)
                (conditional!
                  ((symbol? form) (list form))
                  ((pair? form) (append (symbols (first form)) (symbols (rest form))))
                  (#true ()))))
            (define! private
              (filter (lambda! (s) (not? (or!? (member? s built-in-names) (member? s top-names))))
                      (symbols (read-forms \"runner\" (read-file {runner:?})))))
            (display top-names \" \" (filter (lambda! (s) (binds? here s)) private) \"\\n\"))
          (current-context) (names (current-context)))\n"
    );
    let want = format!("({}) ()\n", documented_operations().join(" "));
    let dir = scratch("reach");
    let (status, out, err) = script(&dir, &probe, &[], "");
    assert_eq!((status, out.as_str()), (Some(0), want.as_str()), "{err}");
    #[cfg(target_os = "linux")]
    {
        let (status, shown) = at_a_terminal(&format!("{probe}\x04"));
        assert_eq!(status, Some(0), "{shown}");
        assert!(shown.contains(&want), "{shown}");
    }
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
    // Also for a program read from stdin.
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
    // A program's own error of that name is only that: it is caught, and uncaught it is reported.
    let forged = "(catch! (raise (error (quote! exit) 3)) e (display \"caught\"))";
    assert_eq!(script(&dir, forged, &[], "").1, "caught");
    let (status, _, err) = script(&dir, "(raise (error (quote! exit) 3))", &[], "");
    assert_eq!(status, Some(1));
    assert!(err.contains("exit 3"), "{err}");
}

#[test]
fn run_process_gives_a_record() {
    let dir = scratch("process");
    let program = r#"
        (define! ran (run-process "sh" "-c" "echo out; echo err >&2; exit 4"))
        (display ran (list (evaluate-in! status ran) (evaluate-in! output ran) (evaluate-in! error-output ran)))
        (display (catch! (run-process "no-such-program-here") e (evaluate-in! kind e)))
        (display (evaluate-in! status (run-process "sh" "-c" "exec >&- 2>&-; sleep 0.2; echo done > finished")))
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(
        out,
        "#[context error-output output status](4 \"out\\n\" \"err\\n\")process-error0"
    );
    // A process that closes its output and goes on is waited for, not killed.
    assert_eq!(
        std::fs::read_to_string(dir.join("finished")).unwrap(),
        "done\n"
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
    std::fs::write(
        dir.join("lib/a.grift"),
        "(define! a 1)\n(load \"lib/b.grift\")\n",
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
        (catch-kind! (load "lib/a.grift") type-error e (write-trace (first (evaluate-in! trace e))))
        (display (b 1) " " loads " ")
        ; Each load runs the file again; in a child context, its definitions are the child's.
        (define! child (context (current-context)))
        (catch! (evaluate (quote! (load "lib/b.grift")) child) #ignore #inert)
        (display loads " " (evaluate (quote! loads) child) " ")
        ; A file that doesn't read runs nothing, and says where the form opened, at the load.
        (catch! (load "lib/bad.grift") e (sequence! (display (evaluate-in! values e)) (write-trace (first (evaluate-in! trace e)))))
        (display (string-ends-with? script-path "script.grift"))
        (load "lib/b.grift")
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    assert_eq!(status, Some(1));
    // A file that doesn't read is innermost in the trace, by its path: the line and column are its.
    assert_eq!(
        out,
        "(first 2) 2 1 1 2 (unclosed-list 3 3)lib/bad.grift #true"
    );
    // Each combination in the trace says its file and line, the loaded file's and the script's.
    let path = dir.join("script.grift");
    let path = path.to_str().unwrap();
    assert_eq!(
        err,
        format!(
            "{path}: type-error 2 in (first 2) at lib/b.grift:4\n  \
             from (define! c (first 2)) at lib/b.grift:4\n  \
             from (load \"lib/b.grift\") at {path}:13\n"
        )
    );
    // Uncaught, it names the file, then the load.
    let (status, _, err) = script(&dir, "(define! d \"lib/bad.grift\")\n(load d)\n", &[], "");
    assert_eq!(status, Some(1));
    assert_eq!(
        err,
        format!(
            "{path}: parse-error unclosed-list 3 3 in lib/bad.grift\n  \
             from (load d) at {path}:2\n"
        )
    );
}

/// A relative path means the same to `load` as to every file operation: relative to the working
/// directory. A file beside the script is named through `script-path`, by the docs' helper.
#[test]
fn load_reads_a_relative_path_from_the_working_directory() {
    let dir = scratch("load-relative");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("lib.grift"), "(define! where \"working\")").unwrap();
    std::fs::write(dir.join("sub/lib.grift"), "(define! where \"beside\")").unwrap();
    std::fs::write(dir.join("data"), "!").unwrap();
    let program = r#"
        (load "lib.grift")
        (display where (read-file "data"))
        (define! beside-script
          (lambda! (path) (string-join (reverse (pair path (rest (reverse (string-split script-path "/"))))) "/")))
        (load (beside-script "lib.grift"))
        (display " " where)
    "#;
    std::fs::write(dir.join("sub/main.grift"), program).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_grift"))
        .arg("sub/main.grift")
        .current_dir(&dir)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        (out.status.code(), &out.stdout[..]),
        (Some(0), &b"working! beside"[..]),
        "{err}"
    );
}

#[test]
fn a_program_s_own_loader_gets_the_lines_load_gets() {
    let dir = scratch("own-loader");
    let program = r#"
        (define! forms (read-forms "data.txt" "\n(a)\nsym\n(b\n (c))\n5"))
        (display (map position forms) (position 1))
        (for-each (lambda! (form) (evaluate form (current-context))) (read-forms "inner.grift" "\n(first 1)"))
    "#;
    let (status, out, err) = script(&dir, program, &[], "");
    assert_eq!(status, Some(1));
    assert_eq!(out, "((\"data.txt\" 2) () (\"data.txt\" 4) ())()");
    assert!(err.contains("in (first 1) at inner.grift:2"), "{err}");
}

#[cfg(target_os = "linux")]
#[test]
fn nested_loads_need_no_rust_stack() {
    let dir = scratch("nested-loads");
    for n in 0..1000 {
        let text = if n == 999 {
            "(define! answer 42)".to_string()
        } else {
            format!("(load \"{}.grift\")", n + 1)
        };
        std::fs::write(dir.join(format!("{n}.grift")), text).unwrap();
    }
    let path = dir.join("script.grift");
    std::fs::write(&path, "(load \"0.grift\") (display answer)").unwrap();
    let out = Command::new("bash")
        .args(["-c", "ulimit -c 0; ulimit -s 512; exec \"$@\"", "load-test"])
        .arg(env!("CARGO_BIN_EXE_grift"))
        .arg(path)
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"42");
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
                       (catch! (put (list 1)) e (evaluate-in! kind e)) " "
                       (catch! (put (list-to-bytes (list 256))) e (evaluate-in! kind e)))
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
        "1000 #bytes() type-error range-error"
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
    let path = dir.join("script.grift");
    let path = path.to_str().unwrap();
    assert_eq!(
        err,
        format!("{path}: parse-error unclosed-list 2 1 in {path}\n")
    );
    let error = run(&[], "(a\n  (b", Stdio::piped());
    assert_eq!(error.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(error.stderr).unwrap(),
        "stdin: parse-error unclosed-list 2 3 in stdin\n"
    );
}

/// A report too long for one write is written once all the same, a piece at a time.
#[test]
fn a_report_over_4_kib_is_written_once() {
    let dir = scratch("long-report");
    let (status, _, err) = script(&dir, "(first (string-repeat \"a\" 5000))", &[], "");
    assert_eq!(status, Some(1));
    let path = dir.join("script.grift");
    let path = path.to_str().unwrap();
    let a = "a".repeat(5000);
    let want =
        format!("{path}: type-error \"{a}\" in (first (string-repeat \"a\" 5000)) at {path}:1\n");
    // Not the text itself on failure: a report written again and again is megabytes.
    assert!(err == want, "{} bytes, not {}", err.len(), want.len());
}

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

/// The write end of a pipe whose read end is gone.
fn closed_pipe() -> std::io::PipeWriter {
    std::io::pipe().unwrap().1
}

#[test]
fn a_closed_stdout_ends_the_run_whatever_catches_the_write() {
    let dir = scratch("closed-stdout");
    for write in ["display", "(evaluate-in! write standard-output)"] {
        let path = dir.join("script.grift");
        let loop_ = format!(
            "(define! go (lambda! () (catch! ({write} \"y\\n\") e 0) (go)))
             (catch! (go) e 0)
             (display-error \"went on\")
             (first 1)"
        );
        std::fs::write(&path, loop_).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_grift"))
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(closed_pipe())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() && start.elapsed().as_secs() < 10 {
            std::thread::yield_now();
        }
        let _ = child.kill();
        let out = child.wait_with_output().unwrap();
        assert_eq!((out.status.code(), &out.stderr[..]), (Some(141), &b""[..]));
    }
}

/// A pipe no one reads ends the run quietly, with 141 as a shell says of a program SIGPIPE ended,
/// so `grift script | head` is quiet and a run never reports a success it didn't have.
#[test]
fn a_closed_pipe_is_a_limit_on_any_file() {
    let dir = scratch("closed-pipe");
    let path = dir.join("script.grift");
    for write in ["display", "(evaluate-in! write standard-output)"] {
        let run = |src: &str| {
            std::fs::write(&path, src).unwrap();
            Command::new(env!("CARGO_BIN_EXE_grift"))
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(closed_pipe())
                .stderr(Stdio::piped())
                .output()
                .unwrap()
        };
        let out = run(&format!("({write} \"y\")"));
        assert_eq!((out.status.code(), &out.stderr[..]), (Some(141), &b""[..]));
        // Its cleanups run, and a program's error in one doesn't replace it.
        let out = run(&format!(
            "(finally! ({write} \"y\") (display-error \"cleanup\") (first 1))"
        ));
        assert_eq!(
            (out.status.code(), &out.stderr[..]),
            (Some(141), &b"cleanup"[..])
        );
        // A closed pipe in the cleanup of an exit is the newer limit, and isn't a success.
        let out = run(&format!("(finally! (exit 3) ({write} \"y\"))"));
        assert_eq!((out.status.code(), &out.stderr[..]), (Some(141), &b""[..]));
        // An exit in a cleanup is the newer limit, and says the status.
        let out = run(&format!("(finally! ({write} \"y\") (exit 3))"));
        assert_eq!(out.status.code(), Some(3));
    }
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

/// Text can't hold a path or an argument that isn't UTF-8, so the runner refuses it, with its bytes,
/// before anything runs: a lossy copy would name another file.
#[cfg(unix)]
#[test]
fn a_path_or_argument_that_is_not_text_is_refused_at_entry() {
    use std::os::unix::ffi::OsStrExt;
    let dir = scratch("not-text");
    let named = |bytes: &[u8]| dir.join(std::ffi::OsStr::from_bytes(bytes));
    std::fs::write(named(b"f\xff"), "(display \"the script given\")").unwrap();
    std::fs::write(named(b"f\xef\xbf\xbd"), "(display \"another file\")").unwrap();
    let good = dir.join("good.grift");
    std::fs::write(&good, "(display \"ran\")").unwrap();
    let not_text = std::ffi::OsStr::from_bytes(b"x\xff");
    for args in [
        vec![named(b"f\xff").into_os_string()],
        vec![good.into_os_string(), not_text.into()],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_grift"))
            .args(&args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            (out.status.code(), &out.stdout[..]),
            (Some(1), &b""[..]),
            "{err}"
        );
        assert!(err.starts_with("grift: encoding-error #bytes("), "{err}");
        assert!(err.ends_with(" 255)\n"), "{err}");
    }
}

#[test]
fn loading_keeps_the_file_s_top_level_form_through_tail_calls() {
    let dir = scratch("loaded-tail-trace");
    let path = dir.join("tail.grift");
    for text in [
        "(define! f (lambda! (x) (first x)))\n(f 2)\n",
        "(define! f (lambda! (x) (first x)))\n(f 2)\n(define! never 1)",
        "\n(if! #true (first 1) 2)",
        "(finally! (first 1) (first 2))",
    ] {
        std::fs::write(&path, text).unwrap();
        let alone = run(&[path.to_str().unwrap()], "", Stdio::piped());
        let load = format!("(load {:?})", path.to_str().unwrap());
        let (status, _, loaded) = script(&dir, &load, &[], "");
        assert_eq!(alone.status.code(), Some(1));
        assert_eq!(status, Some(1));
        let alone = String::from_utf8(alone.stderr).unwrap();
        let alone = alone
            .strip_prefix(&format!("{}: ", path.display()))
            .unwrap()
            .trim_end();
        let wrapper = dir.join("script.grift");
        let loaded = loaded
            .strip_prefix(&format!("{}: ", wrapper.display()))
            .unwrap();
        let loaded = loaded
            .strip_suffix(&format!("\n  from {load} at {}:1\n", wrapper.display()))
            .unwrap();
        assert_eq!(loaded, alone, "{text}");
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

#[cfg(target_os = "linux")]
#[test]
fn a_terminal_session_goes_on_past_an_error_and_ends_with_its_input() {
    let (status, shown) =
        at_a_terminal("(define! x\n 5)\nx\n(first 1)\n)\n(define! first 2)\n7 x\n\x04");
    assert_eq!(status, Some(0), "{shown}");
    assert!(shown.contains("  #inert\n"), "{shown}");
    assert!(
        shown.contains("stdin: type-error 1 in (first 1) at stdin:4\n"),
        "{shown}"
    );
    assert!(
        shown.contains("stdin: parse-error unmatched-parenthesis 5 1 in stdin\n"),
        "{shown}"
    );
    assert!(shown.ends_with("> 5\n> "), "{shown}");
    // Input that ends inside a form is that form's error, and `exit` gives its status.
    let (status, shown) = at_a_terminal("(display\n\x04");
    assert_eq!(status, Some(1), "{shown}");
    assert!(
        shown.contains("stdin: parse-error unclosed-list 1 1"),
        "{shown}"
    );
    assert_eq!(at_a_terminal("(exit 9)\n").0, Some(9));
}

/// A session is a grift program, so an error in its own prompting or reading ends it.
#[cfg(target_os = "linux")]
#[test]
fn closing_what_a_session_prompts_on_or_reads_ends_it_with_status_1() {
    let (status, shown) = at_a_terminal("((evaluate-in! close standard-error))\n1\n\x04");
    assert_eq!(status, Some(1), "{shown}");
    assert!(shown.contains("#inert\n"), "{shown}");
    assert!(
        shown.contains("stdin: closed \"standard-error\""),
        "{shown}"
    );
    assert!(!shown.contains("\n1\n"), "{shown}");
    let (status, shown) = at_a_terminal("((evaluate-in! close standard-input))\n");
    assert_eq!(status, Some(1), "{shown}");
    assert!(
        shown.contains("stdin: closed \"standard-input\""),
        "{shown}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn input_with_no_forms_prints_nothing_at_a_terminal() {
    let (_, shown) = at_a_terminal("\n; a comment\n1\n\x04");
    assert_eq!(shown, "> > 1\n> ");
}

#[cfg(target_os = "linux")]
#[test]
fn a_program_at_a_terminal_reads_the_next_line() {
    let (status, shown) =
        at_a_terminal("((evaluate-in! read-line standard-input))\nafter\n(exit 0)\n");
    assert_eq!(status, Some(0), "{shown}");
    assert!(shown.contains("\"after\\n\"\n"), "{shown}");
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
            "stdin: type-error 1 in (first 1) at stdin:3\n  from (add 1 (f)) at stdin:5\n"
        ),
        "{shown}"
    );
    let (_, shown) = at_a_terminal("1\n2\n)\n\x04");
    assert!(
        shown.contains("stdin: parse-error unmatched-parenthesis 3 1 in stdin\n"),
        "{shown}"
    );
    // So is input ending inside a form.
    let (_, shown) = at_a_terminal("1\n(add 1\n\x04");
    assert!(
        shown.contains("stdin: parse-error unclosed-list 2 1 in stdin\n"),
        "{shown}"
    );
}
