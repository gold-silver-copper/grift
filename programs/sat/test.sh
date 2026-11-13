#!/usr/bin/env bash
# Runs sat on recorded arguments: satisfiable and unsatisfiable sets of clauses, and a bad literal.
# Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=path programs/sat/test.sh
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

check 'sat:1=T,2=T,3=T' '((1) (-1 2) (-2 3))'
check 'unsat' '((1) (-1))'
check 'unsat' '((1 2) (-1 2) (1 -2) (-1 -2))'
check 'sat:1=F,2=T' '((1 -1) (2))'
check 'sat:' '()'
check 'unsat' '(())'
check 'error:sat-literal' '((0))'

echo "$failures failed"
[ "$failures" -eq 0 ]
