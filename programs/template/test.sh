#!/bin/sh
# Runs each case and compares its standard output, standard error and status with the case's
# .expected file. `./test.sh --update` rewrites them; read the diff before committing.
cd "$(dirname "$0")" || exit 2
G=${GRIFT:?set GRIFT to the runner}
failed=0
run() { # NAME ARGUMENT...
    name=$1; shift
    out=$(cd tests && "$G" ../main.grift "$@" 2>&1; echo "[exit $?]")
    if [ "$update" = 1 ]; then printf '%s\n' "$out" > "tests/$name.expected"
    elif ! printf '%s\n' "$out" | diff -u "tests/$name.expected" - > /dev/null; then
        echo "FAIL $name"; printf '%s\n' "$out" | diff -u "tests/$name.expected" -; failed=$((failed + 1))
    fi
}
[ "$1" = --update ] && update=1
run example ../examples/page.html ../examples/data.grift
for t in paths loops cycle-a include-missing no-field no-name not-record if-not-boolean \
         for-not-list no-filter filter-raises insert-list unclosed-tag unclosed-if stray-end \
         wrong-end unknown-tag bad-filter bad-path include-error engine-name; do
    run "$t" "$t.html" data.grift
done
run data-raises count.html data-raises.grift
run data-unread count.html data-unread.grift
run missing-template nope.html data.grift
run usage only-one-argument.html
cases=$(ls tests/*.expected | wc -l)
echo "$((cases - failed)) of $cases cases pass"
[ "$failed" = 0 ]
