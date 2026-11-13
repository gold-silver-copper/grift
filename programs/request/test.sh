#!/usr/bin/env bash
# Runs request on recorded arguments: two requests that parse, and one of each way a request is
# refused. Each must print exactly the line given, nothing on standard error, and exit 0.
#   GRIFT=path programs/request/test.sh
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

check '("GET" "/" ((host "example")) "")' $'GET / HTTP/1.1\r\nHost: example\r\n\r\n'
check '("POST" "/é" ((content-length "2") (host "example")) "é")' $'POST /é HTTP/1.1\r\nHost: example\r\nContent-Length: 2\r\n\r\né'
check '(duplicate-header (host))' $'GET / HTTP/1.1\r\nHost: a\r\nhost: b\r\n\r\n'
check '(body-length-mismatch ((3 1)))' $'POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\n\r\nx'
check '(bad-content-length ("+0"))' $'GET / HTTP/1.1\r\nHost: a\r\nContent-Length: +0\r\n\r\n'
check '(bad-header-name (" bad"))' $'GET / HTTP/1.1\r\nHost: a\r\n bad: x\r\n\r\n'
check '(missing-host ("/"))' $'GET / HTTP/1.1\r\nX: a\r\n\r\n'
check '(bad-host ("two hosts"))' $'GET / HTTP/1.1\r\nHost: two hosts\r\n\r\n'

echo "$failures failed"
[ "$failures" -eq 0 ]
