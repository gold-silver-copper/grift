#!/usr/bin/env bash
# Runs the adventure's checks: the walkthrough reaches the ending, saving and loading keep the
# state, and each broken world stops before play with a message naming what is wrong and where.
# GRIFT names the runner; run from anywhere. The game runs in a temporary directory, where its
# saved game goes.
set -u
here=$(cd "$(dirname "$0")" && pwd)
G=${GRIFT:?set GRIFT to the runner}
work=$(mktemp -d "${TMPDIR:?set TMPDIR}/adventure.XXXXXX") || exit 2
trap 'rm -rf "$work"' EXIT
cp -r "$here/world.grift" "$here/walkthrough.txt" "$here/tests" "$work" || exit 2
cd "$work" || exit 2
failed=0
check() { # name, expected exit status, expected text, command...
  local name=$1 status=$2 text=$3; shift 3
  local out; out=$("$@" 2>&1); local got=$?
  if [ "$got" != "$status" ] || ! grep -qF -- "$text" <<<"$out"; then
    echo "FAIL $name (exit $got, wanted $status and: $text)"; echo "$out" | head -20; failed=1
  else echo "ok   $name"; fi
}
run() { "$G" "$here/main.grift" "$@"; }
check walkthrough 0 '*** The end, in 23 moves. ***' run --echo < walkthrough.txt
check saved-state 0 '(thing oil-can carried)' cat walkthrough.sav
check load-restores 0 'You are carrying a crank handle, an iron key, the keeper'"'"'s logbook, an oil can.' run --echo < walkthrough.txt
check wrong-order 0 'The seized gears' run --echo < walkthrough.txt
check unknown-verb 0 'I don'"'"'t know the verb "dance".' run <<<'dance'
check unknown-word 0 'I don'"'"'t know the word "flurb".' run <<<'take flurb'
check missing-object 0 'Unlock the door with what?' run <<<'unlock door with'
check absent-thing 0 'You don'"'"'t see an iron key here.' run <<<'take key'
check bad-exit 1 'tests/bad-exit.grift:3: place hall: the exit north leads to librery, which is not a place' run tests/bad-exit.grift
check two-places 1 'lamp is in more than one place: hall (tests/bad-exit.grift:3), library (tests/bad-exit.grift:12)' run tests/bad-exit.grift
check bad-door-exit 1 'tests/bad-door.grift:11: the door oak-door closes the exit east of hall, which has no such exit' run tests/bad-door.grift
check bad-door-key 1 'the door oak-door is unlocked by brass-key, which is not an item' run tests/bad-door.grift
check bad-rule 1 'tests/bad-door.grift:15: on! names lantern, which is not an item or a door' run tests/bad-door.grift
check no-start 1 'the world has no (start! place)' run tests/bad-door.grift
check bad-clause 1 'tests/bad-clause.grift:3: place hall: (smell "dust") is not a clause it knows' run tests/bad-clause.grift
check bad-direction 1 'tests/bad-direction.grift:2: place hall: northwest is not a direction' run tests/bad-direction.grift
check defined-twice 1 'tests/bad-twice.grift:4: place hall is defined twice; the first is at tests/bad-twice.grift:2' run tests/bad-twice.grift
check unclosed 1 'tests/bad-syntax.grift:2: this ( is never closed' run tests/bad-syntax.grift
check bad-save 0 'Can'"'"'t load tests/bad-save.sav: tests/bad-save.sav:2: lamp is not an item' run <<<'load tests/bad-save.sav'
exit $failed
