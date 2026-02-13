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
use std::time::Instant;

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

/// Parsed SRFI-64 test run with timing information
#[derive(Debug)]
struct TimedSrfi64Output {
    output: Srfi64Output,
    elapsed: std::time::Duration,
}

/// Split a Scheme source string into top-level expressions.
///
/// Tracks parenthesis nesting depth, handles string literals (including escaped
/// chars), line comments (`;`), block comments (`#| ... |#`), and `#;` datum
/// comments (skips the next expression). Returns a Vec of expression strings.
fn split_top_level_expressions(input: &str) -> Vec<String> {
    let mut exprs = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();
    let mut depth: i32 = 0;

    while let Some(&c) = chars.peek() {
        match c {
            // Line comments - skip to end of line
            ';' => {
                if depth > 0 {
                    // Inside an expression, keep the comment
                    while let Some(&ch) = chars.peek() {
                        current.push(ch);
                        chars.next();
                        if ch == '\n' {
                            break;
                        }
                    }
                } else {
                    // Top-level comment, skip it
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch == '\n' {
                            break;
                        }
                    }
                }
            }
            // Block comments #| ... |#
            '#' if {
                let mut peek = chars.clone();
                peek.next();
                peek.peek() == Some(&'|')
            } =>
            {
                let mut comment = String::new();
                chars.next(); // consume #
                chars.next(); // consume |
                comment.push_str("#|");
                let mut block_depth = 1;
                while block_depth > 0 {
                    match chars.next() {
                        Some('#') if chars.peek() == Some(&'|') => {
                            chars.next();
                            comment.push_str("#|");
                            block_depth += 1;
                        }
                        Some('|') if chars.peek() == Some(&'#') => {
                            chars.next();
                            comment.push_str("|#");
                            block_depth -= 1;
                        }
                        Some(ch) => comment.push(ch),
                        None => break,
                    }
                }
                if depth > 0 {
                    current.push_str(&comment);
                }
            }
            // Datum comment #; - skip next expression
            '#' if {
                let mut peek = chars.clone();
                peek.next();
                peek.peek() == Some(&';')
            } =>
            {
                if depth > 0 {
                    // Inside an expression, keep the #; and the next datum
                    current.push('#');
                    chars.next();
                    current.push(';');
                    chars.next();
                    // Skip whitespace
                    while let Some(&ch) = chars.peek() {
                        if ch.is_whitespace() {
                            current.push(ch);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Now include the next datum in current
                    if chars.peek() == Some(&'(') {
                        let mut d = 1i32;
                        current.push('(');
                        chars.next();
                        while d > 0 {
                            match chars.next() {
                                Some('(') => {
                                    d += 1;
                                    current.push('(');
                                }
                                Some(')') => {
                                    d -= 1;
                                    current.push(')');
                                }
                                Some('"') => {
                                    current.push('"');
                                    loop {
                                        match chars.next() {
                                            Some('\\') => {
                                                current.push('\\');
                                                if let Some(esc) = chars.next() {
                                                    current.push(esc);
                                                }
                                            }
                                            Some('"') => {
                                                current.push('"');
                                                break;
                                            }
                                            Some(ch) => current.push(ch),
                                            None => break,
                                        }
                                    }
                                }
                                Some(ch) => current.push(ch),
                                None => break,
                            }
                        }
                    } else {
                        // Atom - read until whitespace or )
                        while let Some(&ch) = chars.peek() {
                            if ch.is_whitespace() || ch == ')' || ch == '(' {
                                break;
                            }
                            current.push(ch);
                            chars.next();
                        }
                    }
                } else {
                    // Top level #; - skip the next expression entirely
                    chars.next(); // #
                    chars.next(); // ;
                    // Skip whitespace
                    while let Some(&ch) = chars.peek() {
                        if ch.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Skip the next expression
                    if chars.peek() == Some(&'(') {
                        let mut d = 1i32;
                        chars.next();
                        while d > 0 {
                            match chars.next() {
                                Some('(') => d += 1,
                                Some(')') => d -= 1,
                                Some('"') => loop {
                                    match chars.next() {
                                        Some('\\') => {
                                            chars.next();
                                        }
                                        Some('"') => break,
                                        None => break,
                                        _ => {}
                                    }
                                },
                                None => break,
                                _ => {}
                            }
                        }
                    } else {
                        while let Some(&ch) = chars.peek() {
                            if ch.is_whitespace() || ch == ')' || ch == '(' {
                                break;
                            }
                            chars.next();
                        }
                    }
                }
            }
            // String literals
            '"' => {
                current.push('"');
                chars.next();
                loop {
                    match chars.next() {
                        Some('\\') => {
                            current.push('\\');
                            if let Some(esc) = chars.next() {
                                current.push(esc);
                            }
                        }
                        Some('"') => {
                            current.push('"');
                            break;
                        }
                        Some(ch) => current.push(ch),
                        None => break,
                    }
                }
                if depth == 0 {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        exprs.push(trimmed);
                    }
                    current.clear();
                }
            }
            // Open paren
            '(' | '[' => {
                depth += 1;
                current.push(c);
                chars.next();
            }
            // Close paren
            ')' | ']' => {
                depth -= 1;
                current.push(c);
                chars.next();
                if depth == 0 {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        exprs.push(trimmed);
                    }
                    current.clear();
                }
            }
            // Whitespace at top level
            c if c.is_whitespace() && depth == 0 => {
                chars.next();
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    // This is a standalone atom (number, boolean, etc.)
                    exprs.push(trimmed);
                    current.clear();
                }
            }
            // Any other character
            _ => {
                current.push(c);
                chars.next();
            }
        }
    }

    // Don't lose any trailing content
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        exprs.push(trimmed);
    }

    exprs
}

/// Run a single `.scm` test file and return parsed results with timing.
fn run_scheme_test(path: &Path) -> TimedSrfi64Output {
    let start = Instant::now();

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

    // For most test files, wrap in (begin ...) since eval_str handles one
    // expression and (import ...) / (define-library ...) need shared scope.
    // For very large test files (like r7rs-tests.scm), evaluate each top-level
    // expression individually to avoid issues with macro expansion in deeply
    // nested begin contexts and port exhaustion.
    let use_per_expr = path.file_name().map_or(false, |f| f == "r7rs-tests.scm");

    if use_per_expr {
        for expr in split_top_level_expressions(&test_content) {
            let trimmed = expr.trim();
            if trimmed.is_empty() {
                continue;
            }
            eval.eval_str(trimmed).unwrap_or_else(|e| {
                let mut extra = String::new();
                if let Ok(grift::Value::Symbol(name_idx)) = lisp.get(e.expr) {
                    let len = lisp.string_len(name_idx).unwrap_or(0);
                    let mut name = String::new();
                    for i in 0..len {
                        if let Ok(c) = lisp.string_char_at(name_idx, i) {
                            name.push(c);
                        }
                    }
                    extra = format!(" (symbol: '{}')", name);
                }
                panic!(
                    "Failed to evaluate test file: {:?}{}\nFile: {}\nExpression: {}",
                    e,
                    extra,
                    path.display(),
                    if trimmed.len() > 200 { &trimmed[..200] } else { trimmed }
                )
            });
        }
    } else {
        // Default: wrap in (begin ...) for shared scope
        let test_wrapped = format!("(begin\n{}\n)", test_content);
        eval.eval_str(&test_wrapped).unwrap_or_else(|e| {
            let mut extra = String::new();
            if let Ok(grift::Value::Symbol(name_idx)) = lisp.get(e.expr) {
                let len = lisp.string_len(name_idx).unwrap_or(0);
                let mut name = String::new();
                for i in 0..len {
                    if let Ok(c) = lisp.string_char_at(name_idx, i) {
                        name.push(c);
                    }
                }
                extra = format!(" (symbol: '{}')", name);
            }
            panic!(
                "Failed to evaluate test file: {:?}{}\nFile: {}",
                e,
                extra,
                path.display()
            )
        });
    }

    // Parse the captured output
    let output = CAPTURED_OUTPUT.with(|o| o.borrow().clone());
    let elapsed = start.elapsed();
    TimedSrfi64Output {
        output: parse_srfi64_output(&output),
        elapsed,
    }
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
            let timed = run_scheme_test(&path);
            println!(
                "[{:.3}s] {} — {} passed",
                timed.elapsed.as_secs_f64(),
                $file,
                timed.output.passed,
            );
            assert_srfi64_success(&timed.output, &path);
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

// R7RS benchmarks (ported from https://github.com/skyfskyf/r7rs-benchmarks)
srfi64_test!(srfi64_r7rs_bench_fib_test, "r7rs-bench-fib-test.scm");
srfi64_test!(srfi64_r7rs_bench_tak_test, "r7rs-bench-tak-test.scm");
srfi64_test!(
    srfi64_r7rs_bench_cpstak_test,
    "r7rs-bench-cpstak-test.scm"
);
srfi64_test!(
    srfi64_r7rs_bench_primes_test,
    "r7rs-bench-primes-test.scm"
);
srfi64_test!(srfi64_r7rs_bench_sum_test, "r7rs-bench-sum-test.scm");
srfi64_test!(
    srfi64_r7rs_bench_sumfp_test,
    "r7rs-bench-sumfp-test.scm"
);
srfi64_test!(
    srfi64_r7rs_bench_deriv_test,
    "r7rs-bench-deriv-test.scm"
);
srfi64_test!(
    srfi64_r7rs_bench_nqueens_test,
    "r7rs-bench-nqueens-test.scm"
);
srfi64_test!(
    srfi64_r7rs_bench_diviter_test,
    "r7rs-bench-diviter-test.scm"
);
srfi64_test!(
    srfi64_r7rs_bench_divrec_test,
    "r7rs-bench-divrec-test.scm"
);
srfi64_test!(srfi64_r7rs_bench_ack_test, "r7rs-bench-ack-test.scm");
srfi64_test!(
    srfi64_r7rs_bench_fibfp_test,
    "r7rs-bench-fibfp-test.scm"
);
srfi64_test!(
    srfi64_unicode_symbol_tests,
    "unicode-symbol-tests.scm"
);
srfi64_test!(
    chibi_r7rs_tests,
    "r7rs-tests.scm"
);


