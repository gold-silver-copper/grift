#!/usr/bin/env bash
# Runs derive on recorded arguments: derivatives simplified as they are built, and one evaluated at
# a point. Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=path programs/derive/test.sh
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

check '(add (multiply 3 (power x 2)) 2)' '(add (power x 3) (multiply 2 x))' 'x'
check '12.0' '(power x 3)' 'x' '2'
check '(cosine x)' '(sine x)' 'x'
check 'y' '(multiply y x)' 'x'
check '0' '42' 'x'
check '0' '(power 2 0)' 'x'
check '0' '(power 10 30)' 'x'
check '0' '(power y 3)' 'x'

echo "$failures failed"
[ "$failures" -eq 0 ]
