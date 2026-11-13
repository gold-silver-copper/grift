#!/usr/bin/env bash
# Runs forms on recorded arguments: each operative of the little language, a nested local, and its
# errors. Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=/path/to/grift TMPDIR=/a/scratch/directory programs/forms/test.sh
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

check '42' '(local! x 6 (times x (plus x 1)))'
check '7' '(choose! true 7 missing)'
check '5' '(local! x 1 (local! x 2 (plus x 3)))'
check '(this is data)' '(literal! (this is data))'
check '(expected-number (#true))' '(plus true 1)'
check '(unbound-symbol (x))' 'x'

echo "$failures failed"
[ "$failures" -eq 0 ]
