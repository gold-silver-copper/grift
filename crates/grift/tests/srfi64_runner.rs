//! SRFI-64 Test Runner
//!
//! Discovers `*-test.scm` files in `tests/scheme/`, runs them through the
//! Grift evaluator, parses SRFI-64 output, and reports results to `cargo test`.
//!
//! Each `.scm` test file becomes a separate `#[test]` function for parallel
//! execution. Failures show the test file, group, test name, expected vs
//! actual values.

use grift::{Evaluator, Lisp};
use grift_std::StdIoProvider;
use std::cell::RefCell;
use std::path::Path;

/// Parsed SRFI-64 test result
#[derive(Debug)]
enum TestResult {
    #[allow(dead_code)]
    Pass(String),
    Fail {
        name: String,
        expected: String,
        actual: String,
    },
    Error {
        name: String,
        message: String,
    },
}

/// Parsed output from an SRFI-64 test run
#[derive(Debug)]
struct Srfi64Output {
    suite_name: String,
    passed: usize,
    failed: usize,
    errors: usize,
    results: Vec<TestResult>,
}

// Capture output from display/newline via the output callback.
//
// We use a thread-local to accumulate output since the callback is a
// function pointer (not a closure).
thread_local! {
    static CAPTURED_OUTPUT: RefCell<String> = RefCell::new(String::new());
}

fn output_callback<const N: usize>(lisp: &Lisp<N>, val: grift::ArenaIndex) {
    CAPTURED_OUTPUT.with(|output| {
        let mut out = output.borrow_mut();
        if val.is_nil() {
            out.push('\n');
        } else if let Ok(grift::Value::String { .. }) = lisp.get(val) {
            // For display: output string contents without quotes
            let len = lisp.string_len(val).unwrap_or(0);
            for i in 0..len {
                if let Ok(c) = lisp.string_char_at(val, i) {
                    out.push(c);
                }
            }
        } else if let Ok(grift::Value::Char(c)) = lisp.get(val) {
            // For display: output char directly without #\ prefix
            out.push(c);
        } else {
            out.push_str(&format!("{}", lisp.display(val)));
        }
    });
}

/// Run a single `.scm` test file and return parsed results.
fn run_scheme_test(path: &Path) -> Srfi64Output {
    let test_content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));

    // Load SRFI-64 library
    let srfi64_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scheme/srfi-64.scm");
    let srfi64_lib = std::fs::read_to_string(&srfi64_path)
        .unwrap_or_else(|e| panic!("Failed to read SRFI-64 library: {}", e));

    // Clear captured output
    CAPTURED_OUTPUT.with(|output| {
        output.borrow_mut().clear();
    });

    // Create evaluator with large arena (heap-allocated to avoid stack overflow)
    let lisp: Box<Lisp<100000>> = Box::new(Lisp::new());
    let mut eval =
        Evaluator::new(&*lisp).unwrap_or_else(|e| panic!("Failed to create evaluator: {:?}", e));

    // Set up I/O provider for tests that need file/time/port operations
    let mut io = StdIoProvider::new();
    eval.set_io_provider(&mut io);

    // Set up output capture
    eval.set_output_callback(Some(output_callback::<100000>));

    // Load SRFI-64 library (wrap in begin since eval_str handles one expression)
    let lib_wrapped = format!("(begin\n{}\n)", srfi64_lib);
    eval.eval_str(&lib_wrapped).unwrap_or_else(|e| {
        panic!(
            "Failed to load SRFI-64 library: {:?}\nFile: {}",
            e,
            srfi64_path.display()
        )
    });

    // Run the test file (wrap in begin since eval_str handles one expression)
    let test_wrapped = format!("(begin\n{}\n)", test_content);
    eval.eval_str(&test_wrapped).unwrap_or_else(|e| {
        panic!(
            "Failed to evaluate test file: {:?}\nFile: {}",
            e,
            path.display()
        )
    });

    // Parse the captured output
    let output = CAPTURED_OUTPUT.with(|o| o.borrow().clone());
    parse_srfi64_output(&output)
}

/// Parse SRFI-64 output format into structured results.
fn parse_srfi64_output(output: &str) -> Srfi64Output {
    let mut suite_name = String::new();
    let mut passed: usize = 0;
    let mut failed: usize = 0;
    let mut errors: usize = 0;
    let mut results = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("SRFI64:BEGIN ") {
            suite_name = line["SRFI64:BEGIN ".len()..].to_string();
        } else if line.starts_with("SRFI64:PASS ") {
            let name = line["SRFI64:PASS ".len()..].to_string();
            results.push(TestResult::Pass(name));
        } else if line.starts_with("SRFI64:FAIL ") {
            let rest = &line["SRFI64:FAIL ".len()..];
            // Parse: <name> expected:<expected> actual:<actual>
            if let Some(exp_idx) = rest.find(" expected:") {
                let name = rest[..exp_idx].to_string();
                let after_name = &rest[exp_idx + " expected:".len()..];
                if let Some(act_idx) = after_name.find(" actual:") {
                    let expected = after_name[..act_idx].to_string();
                    let actual = after_name[act_idx + " actual:".len()..].to_string();
                    results.push(TestResult::Fail {
                        name,
                        expected,
                        actual,
                    });
                } else {
                    results.push(TestResult::Fail {
                        name,
                        expected: after_name.to_string(),
                        actual: "?".to_string(),
                    });
                }
            } else {
                results.push(TestResult::Fail {
                    name: rest.to_string(),
                    expected: "?".to_string(),
                    actual: "?".to_string(),
                });
            }
        } else if line.starts_with("SRFI64:ERROR ") {
            let rest = &line["SRFI64:ERROR ".len()..];
            if let Some(msg_idx) = rest.find(" message:") {
                let name = rest[..msg_idx].to_string();
                let message = rest[msg_idx + " message:".len()..].to_string();
                results.push(TestResult::Error { name, message });
            } else {
                results.push(TestResult::Error {
                    name: rest.to_string(),
                    message: "unknown".to_string(),
                });
            }
        } else if line.starts_with("SRFI64:SUMMARY ") {
            let rest = &line["SRFI64:SUMMARY ".len()..];
            // Parse: passed:<n> failed:<n> errors:<n>
            for part in rest.split_whitespace() {
                if let Some(val) = part.strip_prefix("passed:") {
                    passed = val.parse().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("failed:") {
                    failed = val.parse().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("errors:") {
                    errors = val.parse().unwrap_or(0);
                }
            }
        }
    }

    Srfi64Output {
        suite_name,
        passed,
        failed,
        errors,
        results,
    }
}

/// Assert that an SRFI-64 test run had no failures or errors.
fn assert_srfi64_success(output: &Srfi64Output, path: &Path) {
    if output.failed > 0 || output.errors > 0 {
        let mut msg = format!(
            "\n══════════════════════════════════════════\n\
             SRFI-64 Test Failure: {}\n\
             Suite: {}\n\
             Passed: {}, Failed: {}, Errors: {}\n\
             ──────────────────────────────────────────\n",
            path.display(),
            output.suite_name,
            output.passed,
            output.failed,
            output.errors
        );
        for result in &output.results {
            match result {
                TestResult::Fail {
                    name,
                    expected,
                    actual,
                } => {
                    msg.push_str(&format!(
                        "  FAIL: {}\n    expected: {}\n    actual:   {}\n",
                        name, expected, actual
                    ));
                }
                TestResult::Error { name, message } => {
                    msg.push_str(&format!("  ERROR: {}\n    message: {}\n", name, message));
                }
                TestResult::Pass(_) => {}
            }
        }
        msg.push_str("══════════════════════════════════════════\n");
        panic!("{}", msg);
    }
    // Also verify we actually ran some tests
    assert!(
        output.passed > 0,
        "No tests were executed in {}",
        path.display()
    );
}

// ============================================================================
// Auto-generated test functions - one per .scm test file
// ============================================================================

macro_rules! srfi64_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/scheme")
                .join($file);
            let output = run_scheme_test(&path);
            assert_srfi64_success(&output, &path);
        }
    };
}

srfi64_test!(srfi64_basics_test, "basics-test.scm");
srfi64_test!(srfi64_arithmetic_test, "arithmetic-test.scm");
srfi64_test!(srfi64_lists_test, "lists-test.scm");
srfi64_test!(srfi64_strings_test, "strings-test.scm");
srfi64_test!(srfi64_control_test, "control-test.scm");
srfi64_test!(srfi64_closures_test, "closures-test.scm");
srfi64_test!(srfi64_bytevector_test, "bytevector-test.scm");
//srfi64_test!(srfi64_continuation_delay_test, "continuation-delay-test.scm");
srfi64_test!(srfi64_environment_test, "environment-test.scm");
srfi64_test!(
    srfi64_first_class_procedures_test,
    "first-class-procedures-test.scm"
);
srfi64_test!(srfi64_issue_test, "issue-test.scm");
srfi64_test!(srfi64_library_test, "library-test.scm");
srfi64_test!(srfi64_peroxide_pitfalls_test, "peroxide-pitfalls-test.scm");
srfi64_test!(srfi64_peroxide_r5rs_test, "peroxide-r5rs-test.scm");
srfi64_test!(srfi64_r5rs_chibi_test, "r5rs-chibi-test.scm");
srfi64_test!(srfi64_r5rs_pitfalls_test, "r5rs-pitfalls-test.scm");
srfi64_test!(srfi64_r7rs_compliance_test, "r7rs-compliance-test.scm");
srfi64_test!(
    srfi64_r7rs_new_procedures_test,
    "r7rs-new-procedures-test.scm"
);
srfi64_test!(srfi64_r7rs_numeric_test, "r7rs-numeric-test.scm");
srfi64_test!(srfi64_io_test, "io-test.scm");
srfi64_test!(srfi64_syntax_extended_test, "syntax-extended-test.scm");
srfi64_test!(
    srfi64_syntactic_extension_html_test,
    "syntactic-extension-html-test.scm"
);
srfi64_test!(srfi64_system_test, "system-test.scm");
srfi64_test!(
    srfi64_syntax_edge_case_test,
    "syntax-edge-case-test.scm"
);
srfi64_test!(srfi64_syntax_proper_test, "syntax-proper-test.scm");

