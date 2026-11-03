#!/usr/bin/env bash
# Checks the test runner's done list by running it. From anywhere:
#   GRIFT=path programs/test/test.sh
# Prints each check and exits 1 if any failed.
cd "$(dirname "$0")" || exit 2
grift=${GRIFT:?set GRIFT to the runner}
work=$(mktemp -d "${TMPDIR:-$HOME/.cache/claude-tmp}/test-app-XXXXXX") || exit 2
trap 'rm -rf "$work"' EXIT
bad=0

# run ARGUMENT...: runs the runner; its stdout and stderr land in $out and $err, its status in $status.
out=$work/out err=$work/err
run() { "$grift" main.grift "$@" >"$out" 2>"$err"; status=$?; }

# expect WHAT CONDITION...: prints ok or FAILED for WHAT.
expect() {
    local what=$1; shift
    if "$@"; then echo "ok      $what"; else echo "FAILED  $what"; bad=1; fi
}
has() { grep -qF -- "$1" "${2:-$out}"; }
hasnt() { ! grep -qF -- "$1" "${2:-$out}"; }
status_is() { [ "$status" = "$1" ]; }
# has_lines LINE...: the report holds these lines one after another.
has_lines() {
    local expected; expected=$(printf '%s\n' "$@")
    python3 -I -c 'import sys; sys.exit(0 if sys.argv[1] in open(sys.argv[2]).read() else 1)' "$expected" "$out"
}

echo "== 1. each test runs alone"
run failing/on-purpose.grift
expect "the tests before, between and after the failures ran" has "failing/on-purpose.grift:4  passes before the failures ... ok"
expect "a raising test doesn't stop the next" has "failing/on-purpose.grift:22  the expression raises, and the test goes on ... FAIL"
expect "a test calling exit doesn't stop the run" has "failing/on-purpose.grift:70  passes after the failures ... ok"
expect "a form that raises ends only its test" has_lines \
    "failing/on-purpose.grift:17  a form raises, ending the test ... FAIL" \
    "     expression: (helper 1)" \
    "     actual:     raised type-error #[primitive first] 1" \
    "       at:       (first x)" \
    "       at:       (add 1 (first x))"
expect "a definition stays in its test" has "failing/on-purpose.grift:67  so this one sees nothing ... ok"
run failing/raises-loading.grift
expect "a form that raises while loading doesn't stop the file" has "failing/raises-loading.grift:17  the last one ... ok"
expect "load! of a missing file names it, with none of the runner's own forms" has_lines \
    '     expression: (load! "no-such-file.grift")' \
    '     actual:     raised file-error "failing/no-such-file.grift" "No such file or directory (os error 2)"' \
    "failing/raises-loading.grift:3  after a raising form ... ok"
run failing/does-not-read.grift tests/numbers.grift
expect "a file that doesn't read names where, and the others run" has_lines \
    "failing/does-not-read.grift:4  when loading ... FAIL" \
    "     actual:     raised parse-error unclosed-list 4 1"
expect "the tests before the bad form still run" has "failing/does-not-read.grift:2  before the bad form ... ok"
expect "the next file still runs" has "tests/numbers.grift:4  integers / divide and remainder truncate toward zero ... ok"
run no-such-file.grift tests/numbers.grift
expect "a missing file names itself, and the others run" has 'raised file-error "no-such-file.grift"'
expect "after a missing file the others run" has "tests/numbers.grift:4  integers / divide and remainder truncate toward zero ... ok"
expect "a missing file fails the run" status_is 1
run failing/stops-the-run.grift
expect "a limit ends the run, reported" has "failing/stops-the-run.grift:4  raises the host's limit ... STOP"
expect "after a limit the summary still prints" has "1 passed, 0 failed, 0 not selected, 0 failed to load"
expect "after a limit the status is 1" status_is 1
timeout 2 "$grift" main.grift failing/never-ends.grift >"$out" 2>"$err"; status=$?
expect "a test that never ends: its name is the last thing shown" bash -c "tail -1 '$out' | grep -q 'never ends ... $'"
expect "a test that never ends is stopped only from outside" status_is 124

echo "== 2. a failure shows the name, the expression, the expected and the actual"
run failing/on-purpose.grift
expect "wrong value" has_lines \
    "failing/on-purpose.grift:7  wrong value ... FAIL" \
    "     expression: (add 1 1)" \
    "     expected:   3" \
    "     actual:     2"
expect "every failed expectation of a test is shown" has_lines \
    "     actual:     (3 2 1)" \
    "     expression: (empty-list? (list 1))"
expect "wrong kind of error" has_lines \
    "     expected:   raises type-error" \
    "     actual:     raised division-by-zero #[primitive divide] 1 0"
expect "a test that checks nothing fails" has "no expectation ran: this test checks nothing"
expect "a failed test's output is shown" has_lines "     | seen 42" "     | after"
expect "values that print alike are explained" has "they print alike but aren't equal"

echo "== 3. summary and status"
run failing/on-purpose.grift
expect "summary with failures" has "4 passed, 18 failed, 0 not selected, 0 failed to load"
expect "status 1 with failures" status_is 1
run tests/*.grift
expect "status 0 when all pass" status_is 0
expect "summary is the last line and says 0 failed" bash -c "tail -1 '$out' | grep -qE '^[0-9]+ passed, 0 failed, 0 not selected, 0 failed to load$'"
expect "at least 20 passing library tests" bash -c "[ \$(grep -c ' \.\.\. ok$' '$out') -ge 20 ]"
run
expect "no files: usage on stderr, status 2" bash -c "[ $status = 2 ] && grep -q '^usage:' '$err'"
run --no-such-option tests/numbers.grift
expect "unknown option: status 2" status_is 2

echo "== 4. selecting by name"
run --name "wrong value" failing/on-purpose.grift
expect "selects by part of the name" has "failing/on-purpose.grift:7  wrong value ... FAIL"
expect "and nothing else" hasnt "passes before"
expect "the summary counts the rest" has "0 passed, 2 failed, 20 not selected"
run --name "passes before" --name "passes after" failing/on-purpose.grift
expect "several --name: any of them; 0 when those pass" status_is 0
expect "several --name: both ran" has "2 passed, 0 failed, 20 not selected"
run tests/program.grift --name "stack / with shared"
expect "a group's name is part of a test's" has "2 passed, 0 failed, 12 not selected"
expect "a test in a group has its own line" has "tests/program.grift:28  stack / with shared setup / a group nests in its name ... ok"
run --name "nothing is called this" tests/numbers.grift
expect "selecting nothing is status 1, said" bash -c "[ $status = 1 ] && grep -q '^no test ran$' '$out'"
run --list --name "stack /" tests/program.grift
expect "--list names tests and runs none" bash -c "grep -q 'stack / what it prints' '$out' && ! grep -q ' ok$' '$out' && ! grep -q 'passed' '$out'"

run tests/program.grift
expect "a computed name doesn't move the lines of later forms" has "tests/program.grift:53  the form after it has its own line ... ok"

echo "== 5. tests of library names, some failing on purpose"
run tests/lists.grift tests/strings.grift tests/control.grift tests/edges.grift tests/errors.grift tests/numbers.grift tests/contexts.grift
echo "        $(tail -1 "$out")"
expect "library tests all pass" status_is 0
run failing/on-purpose.grift
echo "        $(tail -1 "$out")"
expect "the failing ones fail" status_is 1

echo "== testing a whole script"
run tests/real-program.grift
echo "        $(tail -1 "$out")"
expect "count-words.grift, unchanged, under stand-ins for what it takes from the runner" status_is 0

echo "== the runner's own edges"
"$grift" main.grift tests/lists.grift 2>"$err" | head -1 >/dev/null
piped=${PIPESTATUS[0]}
expect "a closed pipe ends it quietly" bash -c "[ $piped = 0 ] && ! grep -q . '$err'"
expect "report on stdout, nothing on stderr" bash -c "'$grift' main.grift tests/numbers.grift 2>&1 >/dev/null | wc -c | grep -qx 0"

[ "$bad" = 0 ] && echo "all checks passed" || echo "SOME CHECKS FAILED"
exit "$bad"
