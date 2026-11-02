#!/bin/sh
# Runs simulate's tests: the unit tests in tests.grift, then the command line. GRIFT is the runner
# GRIFT names the runner.
here=$(cd "$(dirname "$0")" && pwd)
: "${GRIFT:?set GRIFT to the runner}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failed=0
fail() { echo "FAIL $*"; failed=1; }
sim() { "$GRIFT" "$here/main.grift" "$@"; }

"$GRIFT" "$here/tests.grift" "$here/main.grift" || fail "tests.grift"

# A run repeats with its seed, and another seed gives another run.
sim "$here/bank.grift" > "$work/a" || fail "bank.grift exits 0"
sim "$here/bank.grift" > "$work/b"
cmp -s "$work/a" "$work/b" || fail "same seed, same output"
sed 's/(seed 42)/(seed 43)/' "$here/bank.grift" > "$work/other.grift"
sim "$work/other.grift" > "$work/c"
cmp -s "$work/a" "$work/c" && fail "another seed, another output"
for label in events customers "average wait" "longest wait" "average line" "longest line" "teller use" "teller 3"; do
  grep -q "^ *$label " "$work/a" || fail "report has $label"
done

# Events schedule events: the trace shows arrivals and the departures they set off, in time order.
sim "$here/bank.grift" --trace 40 > "$work/trace"
[ "$(awk '$2 == "arrival" || $2 == "departure"' "$work/trace" | wc -l)" = 40 ] || fail "trace prints 40 events"
grep -q departure "$work/trace" || fail "trace has departures"
awk '$2 == "arrival" || $2 == "departure" { if ($1 < last) bad = 1; last = $1 } END { exit bad }' "$work/trace" || fail "trace in time order"

# A long M/M/1 run lands near Erlang's prediction: wait within 10%, teller use within 2 points.
printf '(tellers 1) (arrival-rate 0.5) (service-rate 1.0) (duration 20000.0) (seed 11)\n' > "$work/mm1.grift"
sim "$work/mm1.grift" > "$work/mm1"
awk '/^average wait/ { w = $3; p = $6 } /^teller use/ { u = $3 + 0; q = $5 + 0 }
     END { d = w - p; if (d < 0) d = -d; e = u - q; if (e < 0) e = -e; exit !(d < 0.1 * p && e < 2) }' "$work/mm1" \
  || { fail "M/M/1 near theory"; cat "$work/mm1"; }

# Wrong configs and command lines: a message naming the file and line, status 2.
expect_error() { # config text, expected message part
  printf '%s\n' "$1" > "$work/bad.grift"
  sim "$work/bad.grift" > "$work/out" 2> "$work/err"; status=$?
  [ $status = 2 ] && grep -qF "$2" "$work/err" || fail "config '$1': status $status, $(cat "$work/err")"
}
good='(tellers 3) (arrival-rate 1.0) (service-rate 0.4) (duration 480.0)'
expect_error "$good" "bad.grift: seed is missing"
expect_error "$good (seed 1) (seed 2)" "bad.grift:1: seed is set twice"
expect_error "$good
(seed 1.5)" "bad.grift:2: seed must be an integer"
expect_error "(tellers 0)" "bad.grift:1: tellers must be a positive integer, got 0"
expect_error "(teller 3)" "bad.grift:1: expected (name value) with a name of tellers"
expect_error "(tellers 3) (seed" "bad.grift:2:1: the config doesn't read here"
sim "$work/missing.grift" 2> "$work/err"; [ $? = 2 ] && grep -q "No such file" "$work/err" || fail "missing file"
sim 2> "$work/err"; [ $? = 2 ] && grep -q usage "$work/err" || fail "no arguments"
sim "$here/bank.grift" --trace many 2> "$work/err"; [ $? = 2 ] && grep -q usage "$work/err" || fail "bad --trace"

[ $failed = 0 ] && echo "test.sh: all passed"
exit $failed
