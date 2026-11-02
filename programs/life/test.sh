#!/usr/bin/env bash
# Checks life against Conway's rules, not against saved output: a block stays, a blinker
# oscillates with period 2, a glider moves one cell down and right every 4 generations, and
# bad input is an error naming what is wrong.
#   GRIFT=path programs/life/test.sh
set -u
cd "$(dirname "$0")"
grift=${GRIFT:?set GRIFT to the runner}
work=${TMPDIR:?set TMPDIR}/life-test
mkdir -p "$work"
failures=0
check() { # NAME EXPECTED ACTUAL
    if [ "$2" == "$3" ]; then echo "ok   $1"; else
        echo "FAIL $1"; echo "  expected: $2"; echo "  actual:   $3"; failures=$((failures + 1)); fi
}
life() { "$grift" main.grift "$@"; }

check "block is a still life" "generation 1 repeats generation 0: a still life; stopping" \
    "$(life patterns/block.cells 10 | tail -1)"
check "blinker turns" "$(printf '.....\n.....\n.###.\n.....\n.....')" \
    "$(life --final patterns/blinker.cells 1)"
check "blinker has period 2" "generation 2 repeats generation 0: a cycle of period 2; stopping" \
    "$(life patterns/blinker.cells 10 | tail -1)"

# The grid moved one cell right and one down, wrapping: the last row comes to the top and each
# row's last cell to its front. Away from the edges, that is the glider's move.
moved() { awk '{ row[NR] = substr($0, length($0)) substr($0, 1, length($0) - 1) }
               END { print row[NR]; for (i = 1; i < NR; i++) print row[i] }'; }
glider=$(cat patterns/glider.cells)
check "glider moves 1,1 in 4 generations" "$(echo "$glider" | moved)" \
    "$(life --final patterns/glider.cells 4)"
check "glider moves 2,2 in 8 generations" "$(echo "$glider" | moved | moved)" \
    "$(life --final patterns/glider.cells 8)"
check "bounded glider ends as a block" "generation 16 repeats generation 15: a still life; stopping" \
    "$(life patterns/glider.cells 100 | tail -1)"
check "wrapping glider crosses the edges" "$(echo "$glider" | moved | moved | moved | moved | moved)" \
    "$(life --wrap --final patterns/glider.cells 20)"
check "wrapping glider comes back in 24 generations on 6x6" \
    "generation 24 repeats generation 0: a cycle of period 24; stopping" \
    "$(life --wrap patterns/glider.cells 100 | tail -1)"
# A blinker on the left edge: bounded, the edge cuts its bar short; wrapped, the bar reaches
# round to the right edge.
printf '.....\n#....\n#....\n#....\n.....\n' > "$work/edge.cells"
check "bounded edge cuts" "$(printf '.....\n.....\n##...\n.....\n.....')" "$(life --final "$work/edge.cells" 1)"
check "wrapped edge wraps" "$(printf '.....\n.....\n##..#\n.....\n.....')" "$(life --wrap --final "$work/edge.cells" 1)"
check "size from a flag" "7 rows of 9" \
    "$(life --size 9x7 --final patterns/blinker.cells 0 | awk '{ n++; w = length($0) } END { print n " rows of " w }')"
check "short lines are padded" "$(printf '#.\n..')" \
    "$(printf '#\n..\n' > "$work/short.cells"; life --final "$work/short.cells" 0)"
check "comment lines are skipped" "$(printf '....\n.##.\n.##.\n....')" "$(life --final patterns/block.cells 0)"

status() { "$@" > /dev/null 2> "$work/err"; echo "$? $(cat "$work/err")"; }
check "missing file" "2 life: no-such.cells: No such file or directory (os error 2)" \
    "$(status life no-such.cells 1)"
printf '..#\n.o.\n' > "$work/bad.cells"
check "bad cell names line and column" "1 life: $work/bad.cells:2:2: a cell is . or #, not \"o\"" \
    "$(status life "$work/bad.cells" 1)"
check "pattern larger than the grid" "1 life: patterns/glider.cells: the pattern is 6x6, larger than the grid, 3x3" \
    "$(status life --size 3x3 patterns/glider.cells 1)"
check "bad generations" "2 life: GENERATIONS must be a whole number, not \"ten\"" \
    "$(status life patterns/block.cells ten)"
check "no arguments" "2 life: usage: main.grift [--wrap] [--size WIDTHxHEIGHT] [--final] PATTERN-FILE GENERATIONS" \
    "$(status life)"

echo "$failures failed"
[ "$failures" -eq 0 ]
