#!/bin/sh
# Runs the assembler and the machine on every program here and checks what they say. From the
# repository root or from here: programs/machine/test.sh. GRIFT names the runner.
cd "$(dirname "$0")" || exit 2
grift=${GRIFT:?set GRIFT to the runner}
failures=0
out=$(mktemp) err=$(mktemp)
trap 'rm -f "$out" "$err"' EXIT

fail() { failures=$((failures + 1)); echo "FAILED $1: $2"; echo "  stdout: $(head -c 400 "$out")"; echo "  stderr: $(head -c 400 "$err")"; }

# check NAME EXPECTED-STATUS EXPECTED-STDOUT EXPECTED-TEXT-IN-STDERR ARGUMENT...
check() {
    name=$1 status=$2 stdout=$3 stderr=$4; shift 4
    "$grift" main.grift "$@" >"$out" 2>"$err"; got=$?
    [ "$got" = "$status" ] || { fail "$name" "status $got, wanted $status"; return; }
    [ "$stdout" = "*" ] || [ "$(cat "$out")" = "$stdout" ] || { fail "$name" "wrong stdout"; return; }
    [ -z "$stderr" ] || grep -qF -- "$stderr" "$err" || { fail "$name" "stderr lacks: $stderr"; return; }
}

# Item 3: the programs.
check hello 0 "hello, world" "" run programs/hello.asm
check sum 0 500500 "" run programs/sum.asm
check factorial 0 "$(printf '0! 1\n1! 1\n2! 2\n3! 6\n4! 24\n5! 120\n6! 720\n7! 5040\n8! 40320\n9! 362880\n10! 3628800\n11! 39916800\n12! 479001600')" "" run programs/factorial.asm
# The sort's second line must be the first line sorted, with 100 numbers.
check sort-runs 0 "*" "" run programs/sort.asm
unsorted=$(sed -n 1p "$out" | tr ' ' '\n' | grep . | sort -n | tr '\n' ' ')
sorted=$(sed -n 2p "$out")
[ "$unsorted" = "$sorted" ] || { failures=$((failures + 1)); echo "FAILED sort: second line isn't the first line sorted"; }
[ "$(echo "$sorted" | wc -w)" = 100 ] || { failures=$((failures + 1)); echo "FAILED sort: not 100 numbers"; }

# Item 2: the machine's semantics.
check arithmetic 0 1 "" run tests/arithmetic.asm
check zeroed 0 7 "" run tests/zeroed.asm
check self-modifying 0 "!" "" run tests/self-modifying.asm
check utf8 0 "é€𝄞" "" run tests/utf8.asm
check fault-address 1 "" "fault-address.asm:4: machine fault at 000c: address 65536 is outside memory" run tests/fault-address.asm
check fault-division 1 "" "fault-division.asm:2: machine fault at 0004: division by zero" run tests/fault-division.asm
check fault-illegal 1 "" "machine fault at 0004: illegal instruction, opcode 0" run tests/fault-illegal.asm
check fault-register-read 1 "" "no such register: 9" run tests/fault-register-read.asm
check fault-register-write 1 "" "no such register: 9" run tests/fault-register-write.asm
check fault-byte 1 "" "byte 255 can't be written" run tests/fault-byte.asm
check fault-partial 1 "" "halted in the middle of a UTF-8 character" run tests/fault-partial.asm
check fault-stack 1 "" "is outside memory" run tests/fault-stack.asm

# Item 1: the assembler's listing and its errors, each naming the line.
check listing 0 "*" "" assemble programs/sum.asm
grep -q '^0000  04 00 00 00' "$out" || { failures=$((failures + 1)); echo "FAILED listing: first row"; }
grep -q '^LIMIT  *= 1000$' "$out" || { failures=$((failures + 1)); echo "FAILED listing: a constant in the names"; }
check errors 1 "" "" assemble tests/errors.asm
for line in \
    "errors.asm:2: unknown instruction 'ad'" \
    "errors.asm:3: add takes 3 operands (register, register, register), got 2" \
    "errors.asm:4: expected a register (r0 to r7, or sp), got 'r9'" \
    "errors.asm:5: the immediate 70000 doesn't fit: it must be from -32768 to 32767" \
    "errors.asm:6: undefined name 'nowhere'" \
    "errors.asm:7: 'start' is already defined on line 2" \
    "errors.asm:8: unknown directive '.bytes'" \
    "errors.asm:9: expected a memory operand like [r1+4], got 'r2'" \
    "errors.asm:10: the offset +300 = 300 doesn't fit: it must be from -128 to 127" \
    "errors.asm:11: bad string" \
    "errors.asm:12: 'r1' is a register, so it can't be a name" \
    "errors.asm:13: undefined name 'EARLIER' (a constant or .space can use only the names above it)" \
    "errors.asm:15: '0xZZ' isn't a number" \
    "errors.asm:16: 'ab' isn't one byte" \
    "errors.asm:17: .space can't be negative: -1" \
    "errors.asm:18: the byte 256 doesn't fit: it must be from -128 to 255"
do
    grep -qF -- "$line" "$err" || { failures=$((failures + 1)); echo "FAILED errors: lacks: $line"; }
done
check constants 0 6 "" run tests/constants.asm
check bad-constant 1 "" "bad-constants.asm:1: '3x' can't be a name" assemble tests/bad-constants.asm
grep -qF "bad-constants.asm:2: '' can't be a name" "$err" || { failures=$((failures + 1)); echo "FAILED bad-constant: an empty name"; }
check missing 2 "" "No such file" run tests/no-such-file.asm
check usage 2 "" "usage:" nonsense
check usage-run 2 "" "usage:" run --trace

# Item 4: trace mode. stderr has a line per instruction, stdout only the program's output.
check trace 0 "hello, world" "0000  la r1, 0x0024" run --trace programs/hello.asm
[ "$(grep -c 'r0=' "$err")" = "$("$grift" main.grift run --count programs/hello.asm 2>&1 >/dev/null | cut -d' ' -f1)" ] ||
    { failures=$((failures + 1)); echo "FAILED trace: one line per instruction, as --count says"; }
# An instruction restarted after reading unwritten memory is traced once.
check trace-once 0 7 "" run --trace tests/zeroed.asm
[ "$(grep -c '^0004' "$err")" = 1 ] || { failures=$((failures + 1)); echo "FAILED trace: restarted instruction traced twice"; }
check count 0 500500 "4095 instructions" run --count programs/sum.asm

[ "$failures" = 0 ] && echo "all passed" || { echo "$failures failed"; exit 1; }
