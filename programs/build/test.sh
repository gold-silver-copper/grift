#!/usr/bin/env bash
# Checks build's done list against the example project and a few broken build files.
# GRIFT names the runner; run from anywhere.
set -u
here=$(cd "$(dirname "$0")" && pwd)
grift=${GRIFT:?set GRIFT to the runner}
work=$(mktemp -d "${TMPDIR:?set TMPDIR}/build-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
failures=0

build() { (cd "$dir" && "$grift" "$here/main.grift" "$@") >"$work/out" 2>&1; status=$?; }
expect_status() { [ "$status" = "$1" ] || { echo "FAIL $name: status $status, wanted $1"; cat "$work/out"; failures=$((failures + 1)); }; }
expect_line() { grep -qxF -- "$1" "$work/out" || { echo "FAIL $name: no line: $1"; cat "$work/out"; failures=$((failures + 1)); }; }
expect_no() { ! grep -qF -- "$1" "$work/out" || { echo "FAIL $name: unexpected: $1"; cat "$work/out"; failures=$((failures + 1)); }; }
expect_count() { n=$(wc -l <"$work/out"); [ "$n" = "$1" ] || { echo "FAIL $name: $n lines, wanted $1"; cat "$work/out"; failures=$((failures + 1)); }; }
pass() { [ "$failures" = "$before" ] && echo "ok   $name"; before=$failures; }
before=0

dir="$work/example"
cp -r "$here/example" "$dir"

name="1,5,6 first build: default target, variables, a function, a real C project"
build
expect_status 0
expect_line "cc -O2 -Wall -c -o out/main.o main.c"
expect_line "cc -O2 -Wall -c -o out/greet.o greet.c"
expect_line "cc -O2 -Wall -o greeter out/main.o out/greet.o"
expect_line "sh -c 'sed '\''s/NAME/grift/'\'' banner.in > banner.txt'"
[ "$("$dir/greeter" grift)" = "hello, grift" ] || { echo "FAIL $name: greeter"; failures=$((failures + 1)); }
[ "$(cat "$dir/banner.txt")" = "*** made by grift ***" ] || { echo "FAIL $name: banner"; failures=$((failures + 1)); }
pass

name="2 a second run does nothing"
build
expect_status 0
expect_line "build: all is up to date."
expect_count 1
pass

name="2 touching main.c remakes main.o and the link only"
touch "$dir/main.c"
build
expect_status 0
expect_line "cc -O2 -Wall -c -o out/main.o main.c"
expect_line "cc -O2 -Wall -o greeter out/main.o out/greet.o"
expect_no "greet.o greet.c"
expect_no "sed"
pass

name="2 touching banner.in remakes banner.txt only"
touch "$dir/banner.in"
build banner.txt
expect_status 0
expect_count 1
expect_no "cc "
pass

name="3 -n prints what would run and runs nothing"
touch "$dir/greet.h"
build -n
expect_status 0
expect_line "cc -O2 -Wall -c -o out/greet.o greet.c"
expect_line "cc -O2 -Wall -o greeter out/main.o out/greet.o"
build
expect_line "cc -O2 -Wall -c -o out/greet.o greet.c"
build -n clean
expect_line "rm -rf out greeter banner.txt"
[ -x "$dir/greeter" ] || { echo "FAIL $name: -n clean removed greeter"; failures=$((failures + 1)); }
pass

name="4 a usage error exits 2"
build -x
expect_status 2
expect_line "build: unknown option -x"
pass

broken() {
  name=$1; dir="$work/$2"; mkdir -p "$dir"; cat >"$dir/build.data"
}

broken "4 a cycle names its targets" cycle <<'D'
(rule "a" ("b") ("touch" "a"))
(rule "b" ("c") ("touch" "b"))
(rule "c" ("a") ("touch" "c"))
D
build
expect_status 1
expect_line "build: a cycle: a -> b -> c -> a"
pass

broken "4 a missing input with no rule" missing <<'D'
(rule "out.txt" ("in.txt") ("cp" input target))
D
build
expect_status 1
expect_line "build: no rule to make in.txt, needed by out.txt, and no such file"
pass

broken "4 a failing command shows its output and stops" failing <<'D'
(rule "all" ("first" "second"))
(rule "first" () ((shell "echo making first; echo something broke >&2; exit 3")))
(rule "second" () ("touch" "second"))
D
build
expect_status 1
expect_line "making first"
expect_line "something broke"
expect_line "build: the command for first failed with status 3"
expect_no "touch second"
pass

broken "4 a program that doesn't exist" no-program <<'D'
(rule "x" () ("no-such-program-anywhere" "x"))
D
build
expect_status 1
expect_line "build: can't run no-such-program-anywhere for x: No such file or directory (os error 2)"
pass

broken "build file errors name the line" bad-entry <<'D'
(variable a "1")

(rul "x" () ("true"))
D
build
expect_status 1
expect_line 'build: build.data line 3: an entry is a list starting with variable, function, rule, rules or default, not (rul "x" () ("true"))'
pass

broken "two rules for one target" duplicate <<'D'
(rule "x" () ("true"))
(rule "y" () ("true"))
(rule "x" () ("false"))
D
build
expect_status 1
expect_line "build: build.data line 3: a second rule for x; the first is on line 1"
pass

broken "an error in a function the build file wrote" bad-function <<'D'
(function object-for (source) (string-append source 1))
(rule "x" ((object-for "a.c")) ("true"))
D
build
expect_status 1
expect_line "build: build.data line 2: type-error #[primitive string-append] 1"
pass

broken "an unclosed list" unclosed <<'D'
(variable a "1")
(rule "x" ()
  ("true")
D
build
expect_status 1
expect_line "build: build.data line 2: a ( that is never closed"
pass

if [ "$failures" = 0 ]; then echo "all passed"; else echo "$failures failed"; exit 1; fi
