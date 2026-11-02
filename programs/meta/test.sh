#!/usr/bin/env bash
# Runs meta's tests from the repo root: each program's output against the expected one, the
# programs that print no combiner against grift's own output too, the error reports, the repo's
# count-words.grift under both, and memory for tail calls against a growing recursion.
set -u
cd "$(dirname "$0")/../.."
G=${GRIFT:?set GRIFT to the runner}
M="$G programs/meta/main.grift"
T=${TMPDIR:?set TMPDIR}/meta-test
mkdir -p "$T"
failed=0
check() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1"; diff <(echo "$3") <(echo "$2") | head -20; failed=1; fi; }

for t in basics operatives program; do
    check "$t" "$($M programs/meta/tests/$t.grift 2>&1)" "$(cat programs/meta/tests/$t.out)"
done
for t in basics program; do
    check "$t matches grift" "$($M programs/meta/tests/$t.grift 2>&1)" "$($G programs/meta/tests/$t.grift 2>&1)"
done
for f in programs/meta/tests/errors/*.grift; do
    check "errors/$(basename "$f")" "$($M "$f" 2>&1; echo "exit $?")" "$(cat "${f%.grift}.out")"
done

# The repo's 30-line script, unchanged: stdout and exit status as grift gives them.
printf 'one two three\nfour five\n' > "$T/in.txt"
check "count-words matches grift" \
    "$($M crates/grift-cli/tests/count-words.grift "$T/r" < "$T/in.txt" 2>&1 | sed "s|$T/r|R|"; echo "exit ${PIPESTATUS[0]}")" \
    "$($G crates/grift-cli/tests/count-words.grift "$T/r" < "$T/in.txt" 2>&1 | sed "s|$T/r|R|"; echo "exit ${PIPESTATUS[0]}")"

# meta running itself: the interpreted interpreter runs a program and reports an error.
check "meta on meta" "$($M programs/meta/main.grift programs/meta/tests/square.grift 2>&1)" "meta on meta: 144"
check "meta on meta, an error" "$($M programs/meta/main.grift programs/meta/tests/errors/type.grift 2>&1; echo "exit $?")" \
    "$(cat programs/meta/tests/errors/type.out)"

# Peak memory (KB) of a run.
peak() { python3 -I -c 'import resource,subprocess,sys; subprocess.run(sys.argv[1:], stdout=subprocess.DEVNULL); print(resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)' "$@"; }
small=$(peak $M programs/meta/tests/tail.grift 1000)
large=$(peak $M programs/meta/tests/tail.grift 20000)
grows_small=$(peak $M programs/meta/tests/deep.grift 1000)
grows_large=$(peak $M programs/meta/tests/deep.grift 20000)
echo "     peak KB: tail calls 1000 -> $small, 20000 -> $large; non-tail 1000 -> $grows_small, 20000 -> $grows_large"
check "tail calls in bounded memory" "$([ $((large - small)) -lt 1024 ] && echo bounded)" bounded
check "non-tail recursion grows (the measure works)" "$([ $((grows_large - grows_small)) -gt 4096 ] && echo grows)" grows

exit $failed
