; Wrapping, signs, shifts and bitwise operations. Prints one line of 1 for each check that holds, or the
; number of the check that failed.
ALL_ONES_LOW = 0xFFFF

        ; 0 - 1 wraps to all ones, and is -1 when compared as signed
        li   r0, 0
        li   r1, 1
        sub  r2, r0, r1
        li   r5, 1
        li   r3, -1
        cmp  r2, r3
        jne  failed
        cmpi r2, 0
        jge  failed
        li   r5, 2
        cmp  r2, r0             ; as unsigned, all ones is the largest
        jb   failed
        ; the largest + 1 wraps to 0
        li   r5, 3
        add  r3, r2, r1
        cmpi r3, 0
        jne  failed
        ; 65536 * 65536 wraps to 0, and all ones squared to 1
        li   r5, 4
        la   r3, 65535
        addi r3, 1
        mul  r4, r3, r3
        cmpi r4, 0
        jne  failed
        li   r5, 5
        mul  r4, r2, r2
        cmpi r4, 1
        jne  failed
        ; shifts: 1 << 31 >> 31, and shifting past the top loses bits
        li   r5, 6
        li   r3, 31
        shl  r4, r1, r3
        shr  r4, r4, r3
        cmpi r4, 1
        jne  failed
        li   r5, 7
        li   r3, 4
        shl  r4, r2, r3         ; all ones << 4 is all ones less 15
        li   r3, -16
        cmp  r4, r3
        jne  failed
        ; bitwise
        li   r5, 8
        la   r3, 0xF0F0
        la   r4, 0xFF00
        and  r6, r3, r4
        la   r0, 0xF000
        cmp  r6, r0
        jne  failed
        li   r5, 9
        or   r6, r3, r4
        la   r0, 0xFFF0
        cmp  r6, r0
        jne  failed
        li   r5, 10
        xor  r6, r3, r4
        la   r0, 0x0FF0
        cmp  r6, r0
        jne  failed
        ; words are little-endian, and bytes and words see the same memory
        li   r5, 11
        la   r3, data
        load r4, [r3]
        la   r0, 0x0201
        loadb r6, [r3+1]
        cmpi r6, 2
        jne  failed
        li   r5, 12
        load r6, [r3+4]
        cmpi r6, forward_constant
        jne  failed
        ; a push moves sp by 4 and a pop undoes it
        li   r5, 13
        mov  r6, sp
        push r1
        addi r6, -4
        cmp  r6, sp
        jne  failed
        pop  r6
        cmpi r6, 1
        jne  failed
        ; the characters a comment or an operand split might trip on
        li   r5, 14
        la   r3, punctuation
        loadb r6, [r3]
        cmpi r6, ';'
        jne  failed
        loadb r6, [r3+1]
        cmpi r6, ','
        jne  failed
        li   r0, '1'
        out  r0
        li   r0, '\n'
        out  r0
        halt
failed: li   r0, 'f'
        out  r0
        mov  r0, r5
        addi r0, '0'
        out  r0
        li   r0, '\n'
        out  r0
        halt

data:   .byte 1, 2, 3, 4
        .word forward_constant
punctuation:
        .string ";,"            ; a comment after a string
forward_constant = 40 - 2 * 3 + 'a' - 96
