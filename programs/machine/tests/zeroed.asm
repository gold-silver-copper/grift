; Memory never written reads as zero, a word at a time up to the very last, and the instruction that
; found it unwritten runs once. Prints the sum, 0, and then a 7 that was stored at the top.
        la   r1, 0xFFFC
        load r0, [r1]
        cmpi r0, 0
        jne  bad
        li   r0, 7
        store r0, [r1]
        load r0, [r1]
        addi r0, '0'
        out  r0
        li   r0, '\n'
        out  r0
        halt
bad:    li   r0, 'x'
        out  r0
        halt
