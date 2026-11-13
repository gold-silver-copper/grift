#!/usr/bin/env bash
# Runs message on recorded arguments: messages that survive encoding and decoding, and malformed
# ones. Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=/path/to/grift TMPDIR=/a/scratch/directory programs/message/test.sh
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

check '(("name" "café😀") ("count" -9223372036854775808) ("blob" #bytes(0 255)) ("ok" #true))' 'roundtrip' '(("name" "café😀") ("count" -9223372036854775808) ("blob" #bytes(0 255)) ("ok" #true))'
check '()' 'roundtrip' '()'
check '(("n" 9223372036854775807) ("no" #false) ("empty" ""))' 'roundtrip' '(("n" 9223372036854775807) ("no" #false) ("empty" ""))'
check 'error:message-truncated' 'decode' '00'
check 'error:message-hex' 'decode' 'xx'
check 'error:message-name' 'roundtrip' '(("" 1))'

echo "$failures failed"
[ "$failures" -eq 0 ]
