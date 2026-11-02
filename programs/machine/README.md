# machine: an assembler and a virtual machine for a toy CPU

```sh
grift main.grift assemble FILE.asm                   # a listing: addresses, bytes, source, names
grift main.grift run [--trace] [--count] FILE.asm    # assembles it, loads it at 0, runs it
./test.sh
```

The machine has eight 32-bit registers `r0` to `r7` (`r7` is also `sp`, starting at 65536), 65536 bytes
of memory, zeroed, and a program counter starting at 0. Words are little-endian. The stack grows down.
`--trace` writes each instruction and the registers before it runs to standard error; `--count` the
number of instructions run. A fault or an assembly error exits 1, a usage error or unreadable file 2.

Every instruction is four bytes: an opcode, then its operands' bytes, zero-padded.

| Operands | Instructions |
|---|---|
| none | `halt` `nop` `ret` |
| register | `push` `pop` `out` (writes the low byte to standard output) |
| register, register | `mov` `cmp` |
| register, register, register | `add` `sub` `mul` `div` `rem` `and` `or` `xor` `shl` `shr` (`div`, `rem`, `shr` unsigned; the first is the result) |
| register, immediate (signed 16 bits) | `li` `addi` `cmpi` |
| register, address (unsigned 16 bits) | `la` |
| address | `jmp` `call`; after a `cmp`: `jeq` `jne`, signed `jlt` `jle` `jgt` `jge`, unsigned `jb` `jae` |
| register, memory | `load` `store` (words) `loadb` `storeb` (bytes); memory is `[r1]`, `[r1+4]`, `[r1-4]`, offset signed 8 bits |

A line is optional labels (`name:`), then a constant (`NAME = expression`), an instruction or a directive:
`.byte` `.word` (expressions), `.string` (strings, with grift's escapes, no terminator added), `.space
SIZE`. `;` starts a comment. An expression is `+`, `-` and `*` of decimal or `0x` numbers, characters
(`'a'`, `'\n'`), names and `.`, the address here. A constant and `.space` can use only names above them;
instructions and data can use any.
