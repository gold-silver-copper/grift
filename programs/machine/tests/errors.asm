; every line below the labels is wrong in its own way
start:  ad   r1, r2
        add  r1, r2
        add  r1, r2, r9
        li   r1, 70000
        jmp  nowhere
start:  nop
        .bytes 1
        load r1, r2
        load r1, [r2+300]
        .string "unfinished
r1:     nop
LATER = EARLIER + 1
EARLIER = 2
        li   r1, 0xZZ
        li   r1, 'ab'
        .space -1
        .byte 256
