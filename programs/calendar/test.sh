#!/usr/bin/env bash
# Runs calendar on recorded arguments: leap years, weekdays, month ends, business days around a
# holiday, and bad dates. Each must print exactly the line given, nothing on standard error, and
# exit 0.
#   GRIFT=path programs/calendar/test.sh
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

check '2000-03-01' 'add' '2000-02-28' '2'
check '1900-03-01' 'add' '1900-02-28' '1'
check 'Thursday' 'weekday' '2026-11-12'
check '2' 'between' '2024-02-28' '2024-03-01'
check '2024-01-31,2024-02-29,2024-03-31,2024-04-30' 'monthly' '2024-01-31' '4'
check '2026-11-17' 'business' '2026-11-13' '1' '2026-11-16'
check '2026-11-13' 'business' '2026-11-16' '-1'
check 'error:calendar-range' 'add' '0001-01-01' '-1'
check 'error:calendar-date' 'weekday' '1900-02-29'
check 'yes' 'leap' '2000-01-01'

echo "$failures failed"
[ "$failures" -eq 0 ]
