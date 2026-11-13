#!/usr/bin/env bash
# Runs derive on recorded arguments: derivatives simplified as they are built, and one evaluated at
# a point. Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=path programs/derive/test.sh
set -u
cd "$(dirname "$0")"
grift=${GRIFT:?set GRIFT to the runner}
failures=0
check() { # EXPECTED-LINE ARGUMENT...
    local want=$1 got
    shift
    got=$("$grift" main.grift "$@" 2>&1; echo "exit $?")
    if [ "$got" == "$want"$'\n'"exit 0" ]; then echo "ok   $*"; else
        echo "FAIL $*"; echo "  expected: $want"; echo "  actual:   $got"; failures=$((failures + 1)); fi
}

check '(add (multiply 3 (power x 2)) 2)' '(add (power x 3) (multiply 2 x))' 'x'
check '12.0' '(power x 3)' 'x' '2'
check '(cosine x)' '(sine x)' 'x'
check 'y' '(multiply y x)' 'x'
check '0' '42' 'x'

echo "$failures failed"
[ "$failures" -eq 0 ]
