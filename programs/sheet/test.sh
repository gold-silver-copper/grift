#!/usr/bin/env bash
# Runs sheet's done list against budget.sheet and checks the output. Usage:
#   GRIFT=path programs/sheet/test.sh
set -u
here=$(cd "$(dirname "$0")" && pwd)
grift=${GRIFT:?set GRIFT to the runner}
work=$(mktemp -d "${TMPDIR:?set TMPDIR}/sheet-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
cp "$here/budget.sheet" "$work/budget.sheet"
failures=0
run() { "$grift" "$here/main.grift" "$work/budget.sheet"; }
expect() { # expect NAME OUTPUT LINE...
    local name=$1 out=$2; shift 2
    for line in "$@"; do
        if ! grep -qxF -- "$line" <<<"$out"; then
            echo "FAIL $name: no line: $line"; failures=$((failures + 1))
        fi
    done
    echo "checked $name"
}

# 1. The sheet loads; the grid prints aligned, integers and floats formatted.
out=$(printf 'print\n' | run)
expect "1 grid" "$out" \
  "  A    B       C" \
  "1 Rent    1200      3" \
  "2 Food  310.50 518.50" \
  "3 Bus       45" \
  "4      1555.50" \
  "5      1866.60"

# 2. Setting a cell recalculates the cells that depend on it, and only those, in order.
out=$(printf 'set B3 50\nset A3 "Train"\nshow B5\n' | run)
expect "2 recalculation" "$out" \
  "recalculated B3 B4 B5 C1 C2" \
  "recalculated A3" \
  "B5 (with-tax B4) => 1872.6"

# A changed formula leaves the cells it no longer reads; clearing a cell recalculates its readers.
out=$(printf 'set B4 (add B1 1)\nset B2 999.0\nclear B1\nshow B4\n' | run)
expect "2 graph edges" "$out" \
  "recalculated B4 B5" \
  "recalculated B2 C1 C2" \
  "recalculated B4 B5 C1 C2" \
  "B4 (add B1 1) => error empty-cell (B1)"

# 3. A cycle is an error naming its cells; the rest of the sheet still works.
out=$(printf 'set D1 (add D2 1)\nset D2 (multiply D1 2)\nset D3 (add D1 1)\nshow D1\nshow D3\nset B1 1000\nshow B4\nset D2 5\nshow D1\n' | run)
expect "3 cycle" "$out" \
  "D1 (add D2 1) => error cycle (D1 D2)" \
  "D3 (add D1 1) => error error-in-cell (D1)" \
  "B4 (add (integer-to-float B1) (add B2 (integer-to-float B3))) => 1355.5" \
  "recalculated D2 D1 D3" \
  "D1 (add D2 1) => 6"

# 4. An error in one cell's formula shows in that cell only.
out=$(printf 'set E1 (divide 1 0)\nset E2 (add 1 "x")\nset E3 (add 1 2\nset E4 (nonsense 1)\nprint\nshow E1\nshow E2\nshow E3\n' | run)
expect "4 errors" "$out" \
  "  A    B       C      D E" \
  "1 Rent    1200      3   #division-by-zero" \
  "2 Food  310.50 518.50   #type-error" \
  "3 Bus       45          #formula-error" \
  "4      1555.50          #unbound-symbol" \
  "5      1866.60" \
  "E1 (divide 1 0) => error division-by-zero (1 0)" \
  'E2 (add 1 "x") => error type-error ("x")' \
  'E3 (add 1 2 => error formula-error ("does not read: unclosed-list" "(add 1 2" 0)'

# 5. Range functions over a column and a rectangle, and a function from the sheet file.
out=$(printf 'set D1 (sum B1:B3)\nset D2 (count A1:B3)\nset D3 (average B1:C2)\ndefine (define! half (lambda! (x) (divide x 2.0)))\nset D4 (half (with-tax 10.0))\nshow D1\nshow D2\nshow D3\nshow D4\n' | run)
expect "5 ranges and functions" "$out" \
  "D1 (sum B1:B3) => 1555.5" \
  "D2 (count A1:B3) => 6" \
  "D3 (average B1:C2) => 508.0" \
  "D4 (half (with-tax 10.0)) => 6.0"

# 6. Saving writes the sheet back, and loading it again gives the same values.
before=$(printf 'set D1 (add B1 B3)\nset A6 "say \\"hi\\""\nclear C2\nprint\nsave\n' | run)
after=$(printf 'print\n' | run)
grid() { sed -n '/^  A/,/^saved/p' <<<"$1" | grep -v '^saved'; }
if [ "$(grid "$before")" = "$after" ]; then echo "checked 6 save and load"; else
    echo "FAIL 6: grid differs after reload"; diff <(grid "$before") <(echo "$after"); failures=$((failures + 1)); fi
expect "6 file" "$(cat "$work/budget.sheet")" 'A6 "say \"hi\""' "D1 (add B1 B3)"

[ "$failures" -eq 0 ] && echo "all passed" || { echo "$failures failed"; exit 1; }
