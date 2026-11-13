#!/usr/bin/env bash
# Runs tasks on recorded arguments: dependencies built once in order, a body that sees its own
# definitions, and the errors. Each must print exactly the line given, nothing on standard error,
# and exit 0.
#   GRIFT=/path/to/grift TMPDIR=/a/scratch/directory programs/tasks/test.sh
set -u
cd "$(dirname "$0")"
grift=${GRIFT:?set GRIFT to the runner}
errors=$(mktemp "${TMPDIR:?set TMPDIR}/program-test.XXXXXX")
trap 'rm -f "$errors"' EXIT
failures=0
check() { # EXPECTED-LINE ARGUMENT...
    local want=$1 got
    shift
    got=$("$grift" main.grift "$@" 2> "$errors"; echo "exit $?")
    if [ "$got" == "$want"$'\n'"exit 0" ] && [ ! -s "$errors" ]; then echo "ok   $want"; else
        echo "FAIL $(printf "%q " "$@")"; echo "  expected: $want"; echo "  actual:   $got"; sed "s/^/  standard error: /" "$errors"; failures=$((failures + 1)); fi
}

check 'a,b,c;done=3' '(task! a () (say "a")) (task! b (a) (say "b")) (task! c (a b) (say "c"))' 'c'
check 'lexical;done=1' '(define! text "lexical") (task! a () (say text))' 'a'
check 'error:build-cycle' '(task! a (b)) (task! b (a))' 'a'
check 'error:build-missing' '(task! a (missing))' 'a'
check 'error:type-error' '(task! a () (first 1))' 'a'

echo "$failures failed"
[ "$failures" -eq 0 ]
