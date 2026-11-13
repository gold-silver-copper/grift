#!/usr/bin/env bash
# Runs bignum on recorded arguments: each operation on numbers wider than 64 bits, and the errors.
# Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=path programs/bignum/test.sh
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

check '1000000000000000000000000000000' 'add' '999999999999999999999999999999' '1'
check '-999999999999999999997' 'subtract' '-1000000000000000000000' '-3'
check '1219326311370217952237463801111263526900' 'multiply' '12345678901234567890' '98765432109876543210'
check '-100000000000000000000,-1' 'divide' '-1000000000000000000001' '10'
check '-2,1' 'divide' '7' '-3'
check '1267650600228229401496703205376' 'power' '2' '100'
check '265252859812191058636308480000000' 'factorial' '30'
check '2469135780000000000' 'gcd' '12345678900000000000' '9876543120000000000'
check '0' 'negate' '-0000'
check '-1' 'compare' '-10000000000000000000' '-2'
check 'error:decimal-division-by-zero' 'divide' '10' '0'
check 'error:decimal-digit' 'add' '12x' '3'
check 'error:decimal-negative-power' 'power' '3' '-1'
check '99999999999999999999' 'sqrt' '9999999999999999999999999999999999999999'
check '49382715600000000000' 'lcm' '12345678900000000000' '9876543120000000000'

echo "$failures failed"
[ "$failures" -eq 0 ]
