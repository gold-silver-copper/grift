#!/usr/bin/env bash
# Checks format's done list. Run from anywhere: programs/format/test.sh
# GRIFT names the runner.
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
grift=${GRIFT:?set GRIFT to the runner}
format="$grift $here/main.grift"
work=$(mktemp -d "${TMPDIR:?set TMPDIR}/format-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
failures=0
pass() { echo "ok   $*"; }
fail() { echo "FAIL $*"; failures=$((failures + 1)); }

# Comments as "T text" (trailing: code before it on its line) or "O text" (own line), in order:
# a scan of its own, outside strings (which may span lines), independent of the formatter's.
comment_places() {
    awk '{ code = ""; c = ""
           for (i = 1; i <= length($0); i++) {
               ch = substr($0, i, 1)
               if (instr) { if (ch == "\\") i++; else if (ch == "\"") instr = 0 }
               else if (ch == "\"") instr = 1
               else if (ch == ";") { c = substr($0, i); break }
               else if (ch != " " && ch != "\t") code = code ch
           }
           if (c != "") { sub(/[ \t]+$/, "", c); print (code == "" ? "O " : "T ") c } }' "$1"
}

# 1. Every .grift file in the repo reads back to the same forms, and formatting twice changes nothing.
cd "$repo"
for f in $( (git ls-files '*.grift'; ls programs/format/*.grift programs/format/tests/*.grift) | sort -u); do
    case $f in programs/format/tests/bad.grift | programs/format/tests/stray.grift) continue ;; esac
    out="$work/$(echo "$f" | tr / _)"
    if ! $format "$f" > "$out.1"; then fail "1 $f: format failed"; continue; fi
    if ! $grift "$here/tests/same-forms.grift" "$f" "$out.1"; then fail "1 $f: forms changed"; continue; fi
    $format "$out.1" > "$out.2"
    if cmp -s "$out.1" "$out.2"; then pass "1 $f reads back the same and is stable"; else fail "1 $f: second pass changed it"; fi
    # 2. Comments kept, in order, each still trailing or on its own line.
    if [ "$(comment_places "$f")" = "$(comment_places "$out.1")" ]; then
        pass "2 $f keeps its $(comment_places "$f" | wc -l) comments in place"
    else
        fail "2 $f: comments moved"; diff <(comment_places "$f") <(comment_places "$out.1") | head -5
    fi
    # 3. No line of code goes past 100 columns (comments, which aren't broken, may).
    long=$(grep -v ';' "$out.1" | awk 'length($0) > 100' | wc -l)
    if [ "$long" = 0 ]; then pass "3 $f has no code line over 100"; else fail "3 $f: $long code lines over 100"; fi
done

# 3. Short forms stay on one line; long ones break by the rule.
t="$work/tricky"
$format "$here/tests/tricky.grift" > "$t"
grep -qx '(define! x 1) ; trailing after a short form' "$t" && pass "3 a short form stays on one line" || fail "3 short form"
grep -qx '(newline)' "$t" && pass "3 spaces inside a short form go" || fail "3 (newline)"
if grep -qx '  (lambda! (alpha beta gamma)' "$t" && grep -qx '    (if! (less-than? alpha beta)' "$t" &&
    grep -qx '      (add alpha (multiply beta gamma))' "$t"; then
    pass "3 a long form breaks: operative bodies indented 2, first operand kept"
else fail "3 long form layout"; fi
if grep -q '^  (list "a ; not a comment"$' "$t" && grep -q '^        "paren ( inside"$' "$t"; then
    pass "3 a long call lines up its arguments under the first"
else fail "3 call layout"; fi

# 4. A file that doesn't read is an error naming where, and is left alone.
for f in bad stray; do
    cp "$here/tests/$f.grift" "$work/$f.grift"
    message=$($format --write "$work/$f.grift" 2>&1 >/dev/null); status=$?
    if [ $status = 1 ] && cmp -s "$here/tests/$f.grift" "$work/$f.grift"; then
        pass "4 $f.grift: exit 1, left alone: $message"
    else fail "4 $f.grift: exit $status, $message"; fi
done
echo "$message" | grep -q 'stray.grift:1:14: does not read' || fail "4 stray.grift place"
$format "$here/tests/bad.grift" 2>&1 | grep -q 'bad.grift:4:1: does not read; the list at 2:17 is not closed' &&
    pass "4 an unclosed list is named where it opens" || fail "4 unclosed list message"

# 5. --check names the files that would change and exits 1; --write fixes them.
cp "$here/tests/tricky.grift" "$here/tests/formatted.grift" "$work/"
listed=$($format --check "$work/tricky.grift" "$work/formatted.grift"); status=$?
if [ $status = 1 ] && [ "$listed" = "$work/tricky.grift" ]; then pass "5 --check: exit 1, names only tricky.grift"
else fail "5 --check: exit $status, listed: $listed"; fi
$format --write "$work/tricky.grift" && cmp -s "$work/tricky.grift" "$t" && pass "5 --write rewrites it" || fail "5 --write"
listed=$($format --check "$work/tricky.grift" "$work/formatted.grift"); status=$?
[ $status = 0 ] && [ -z "$listed" ] && pass "5 --check after --write: exit 0, nothing listed" || fail "5 --check after write: $status $listed"
$format --check "$work/nope.grift" 2>/dev/null; [ $? = 1 ] && pass "5 a missing file is an error" || fail "5 missing file"
$format 2>/dev/null; [ $? = 2 ] && pass "usage error exits 2" || fail "usage"

echo "$failures failures"
[ $failures = 0 ]
