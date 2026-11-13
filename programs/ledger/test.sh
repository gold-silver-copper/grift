#!/usr/bin/env bash
# Runs ledger on recorded arguments: a committed batch, and failed ones that leave every balance as
# it was and the lock released. Each must print exactly the line given, nothing on standard error,
# and exit 0.
#   GRIFT=path programs/ledger/test.sh
set -u
cd "$(dirname "$0")"
grift=${GRIFT:?set GRIFT to the runner}
failures=0
check() { # EXPECTED-LINE ARGUMENT...
    local want=$1 got
    shift
    got=$("$grift" main.grift "$@" 2>&1; echo "exit $?")
    if [ "$got" == "$want"$'\n'"exit 0" ]; then echo "ok   $want"; else
        echo "FAIL $(printf "%q " "$@")"; echo "  expected: $want"; echo "  actual:   $got"; failures=$((failures + 1)); fi
}

check '(committed ((alice 70) (bob 40) (reserve 10)) 120 ((locked #false) (released 1)))' '((alice bob 30) (bob reserve 10))'
check '(insufficient-funds ((alice 100) (bob 20) (reserve 0)) 120 ((locked #false) (released 1)))' '((alice bob 30) (bob reserve 999))'
check '(committed ((alice 100) (bob 20) (reserve 0)) 120 ((locked #false) (released 1)))' '((alice alice 12))'
check '(unknown-account ((alice 100) (bob 20) (reserve 0)) 120 ((locked #false) (released 1)))' '((alice nobody 1))'
check '(negative-transfer ((alice 100) (bob 20) (reserve 0)) 120 ((locked #false) (released 1)))' '((alice bob -1))'

echo "$failures failed"
[ "$failures" -eq 0 ]
