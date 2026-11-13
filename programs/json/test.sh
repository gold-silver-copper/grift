#!/usr/bin/env bash
# Runs json on recorded arguments: documents printed back compactly, and each kind of syntax error
# with its offset. Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=path programs/json/test.sh
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

check '{"x":[true,false,null,-0,12.50e+03]}' ' { "x": [true, false, null, -0, 12.50e+03] } '
check '"😀 café\n\b\f\r\t/\\\"\u0000"' '"\uD83D\uDE00 café\n\b\f\r\t\/\\\"\u0000"'
check '[{},[],"",123456789012345678901234567890]' '[{},[],"",123456789012345678901234567890]'
check 'error:leading-zero:1' '01'
check 'error:expected-digit:3' '1e+'
check 'error:expected-value:3' '[1,]'
check 'error:bad-surrogate:7' '"\uDE00"'
check 'error:trailing-data:5' 'true false'
check 'error:expected-value:0' ''
check '{"x":1,"x":2}' '{"x":1,"x":2}'

echo "$failures failed"
[ "$failures" -eq 0 ]
