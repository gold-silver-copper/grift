#!/usr/bin/env bash
# Runs avl on recorded arguments: inserts, deletes, undo and the two queries, checked against the
# tree invariants as it goes. Each must print exactly the line given, nothing on standard error, and
# exit 0.
#   GRIFT=path programs/avl/test.sh
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

check '1:a,2:b,3:c;queries=2,(2 "b")' '((insert 3 "c") (insert 1 "a") (insert 2 "b") (rank 3) (select 1))'
check '1:a,3:c;queries=' '((insert 1 "a") (insert 2 "b") (insert 3 "c") (delete 2))'
check '2:a;queries=' '((insert 2 "a") (insert 2 "new") (undo) (delete 9))'
check '-9223372036854775808:min,9223372036854775807:max;queries=1' '((insert -9223372036854775808 "min") (insert 9223372036854775807 "max") (rank 0))'
check 'error:tree-index' '((select 0))'
check ';queries=' '()'

echo "$failures failed"
[ "$failures" -eq 0 ]
