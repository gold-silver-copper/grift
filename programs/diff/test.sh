#!/usr/bin/env bash
# Checks programs/diff against GNU diff and patch. Every case: the exit status (0 same, 1 different),
# `patch` turning OLD into NEW with our output, and the number of changed lines equal to
# `diff --minimal`'s (both are shortest edit scripts). Then a missing file, usage, and the
# 2,000-line timing. GRIFT names the runner; run from anywhere.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
grift=${GRIFT:?set GRIFT to the runner}
work=$(mktemp -d "${TMPDIR:?set TMPDIR}/diff-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
failures=0
cases=0

fail() { echo "FAIL $*"; failures=$((failures + 1)); }

# check NAME: diffs $work/old and $work/new and checks the result.
check() {
    cases=$((cases + 1))
    "$grift" "$here/main.grift" "$work/old" "$work/new" > "$work/ours" 2> "$work/err"
    local status=$? want=0
    cmp -s "$work/old" "$work/new" || want=1
    [ "$status" = "$want" ] || { fail "$1: exit $status, want $want: $(head -3 "$work/err")"; return; }
    if [ "$want" = 0 ]; then
        [ -s "$work/ours" ] && fail "$1: printed something for identical files"
        return
    fi
    cp "$work/old" "$work/patched"
    patch -s --no-backup-if-mismatch "$work/patched" < "$work/ours" > "$work/patch-out" 2>&1 \
        || { fail "$1: patch refused: $(head -3 "$work/patch-out")"; return; }
    cmp -s "$work/patched" "$work/new" || { fail "$1: patch gave a different file"; return; }
    local ours gnu
    ours=$(tail -n +3 "$work/ours" | grep -c '^[-+]')
    gnu=$(diff --minimal -u "$work/old" "$work/new" | tail -n +3 | grep -c '^[-+]')
    [ "$ours" = "$gnu" ] || fail "$1: $ours changed lines, diff --minimal has $gnu"
}

# Edge cases.
edge() { printf "$2" > "$work/old"; printf "$3" > "$work/new"; check "$1"; }
edge "both empty" "" ""
edge "identical" 'a\nb\n' 'a\nb\n'
edge "old empty" "" 'a\nb\nc\n'
edge "new empty" 'a\nb\nc\n' ""
edge "one line changed" 'a\n' 'b\n'
edge "newline added at end" 'a\nb' 'a\nb\n'
edge "newline removed at end" 'a\nb\n' 'a\nb'
edge "last line changed, no newline" 'a\nb' 'a\nc'
edge "repeated lines" 'a\na\na\nb\na\na\n' 'a\nb\na\nb\na\n'
edge "insert at start" 'b\nc\n' 'a\nb\nc\n'
edge "insert at end" 'a\nb\n' 'a\nb\nc\n'
edge "blank lines" '\n\n\nx\n\n' '\nx\n\n\n\n'
edge "unicode" 'é\nλ\n🙂\n' 'é\nμ\n🙂\n'
edge "carriage returns" 'a\r\nb\r\n' 'a\nb\r\n'
edge "far apart changes" "$(seq 1 30 | tr '\n' '|' | sed 's/|/\\n/g')" \
     "$(seq 1 30 | sed 's/^2$/two/;s/^29$/twenty-nine/' | tr '\n' '|' | sed 's/|/\\n/g')"
edge "changes 6 apart share a hunk" "$(seq 1 20 | tr '\n' '|' | sed 's/|/\\n/g')" \
     "$(seq 1 20 | sed 's/^5$/five/;s/^12$/twelve/' | tr '\n' '|' | sed 's/|/\\n/g')"
edge "changes 7 apart don't" "$(seq 1 20 | tr '\n' '|' | sed 's/|/\\n/g')" \
     "$(seq 1 20 | sed 's/^5$/five/;s/^13$/thirteen/' | tr '\n' '|' | sed 's/|/\\n/g')"

# Random cases: small alphabets make many equal lines, the hard case for a shortest script.
for seed in $(seq 1 40); do
    awk -v seed="$seed" 'BEGIN { srand(seed); n = int(rand() * 40)
        for (i = 0; i < n; i++) print substr("abcde", int(rand() * 5) + 1, 1) }' > "$work/old"
    awk -v seed="$seed" 'BEGIN { srand(seed + 1000) } { r = rand()
        if (r < 0.15) next; if (r < 0.3) print "x"; print; if (r > 0.9) print substr("abcde", int(rand() * 5) + 1, 1) }' \
        "$work/old" > "$work/new"
    check "random $seed"
done

# A missing file: a message on standard error and exit 2.
"$grift" "$here/main.grift" "$work/old" "$work/missing" > "$work/ours" 2> "$work/err"
status=$?
[ "$status" = 2 ] && grep -q "^diff: $work/missing: " "$work/err" && [ ! -s "$work/ours" ] \
    || fail "missing file: exit $status, stderr $(cat "$work/err")"
"$grift" "$here/main.grift" "$work/old" > /dev/null 2> "$work/err"
status=$?
[ "$status" = 2 ] && grep -q "usage" "$work/err" || fail "usage: exit $status"

# Two 2,000-line files with 50 scattered changes (changed, deleted and inserted lines).
awk 'BEGIN { for (i = 1; i <= 2000; i++) printf "line %d of the old file: %d\n", i, (i * 7919) % 1000 }' > "$work/old"
awk -v seed=7 'BEGIN { srand(seed); for (c = 0; c < 50; c++) { p = int(rand() * 2000) + 1; t[p] = int(rand() * 3) } }
    { if (!(NR in t)) { print; next }; k = t[NR]
      if (k == 0) print $0 " (changed)"; else if (k == 2) { print; print "an inserted line after " NR } }' \
    "$work/old" > "$work/new"
start=$(date +%s%N)
check "2,000 lines, 50 changes"
end=$(date +%s%N)
echo "2,000 lines, 50 changes: $(( (end - start) / 1000000 )) ms (with the patch and GNU checks)"
cmp -s <(tail -n +3 "$work/ours") <(diff -u "$work/old" "$work/new" | tail -n +3) \
    && echo "2,000 lines: hunks byte-identical to GNU diff -u" || echo "2,000 lines: hunks differ from GNU diff -u (allowed)"

echo "$cases cases, $failures failures"
[ "$failures" = 0 ]
