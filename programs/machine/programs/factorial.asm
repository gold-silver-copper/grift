; Recursive factorial: prints n! for n = 0..12, the largest that fit in 32 bits.
LAST = 12

        li   r4, 0              ; n
next:   mov  r0, r4
        call print_number
        li   r1, '!'
        out  r1
        li   r1, ' '
        out  r1
        mov  r0, r4
        call factorial
        call print_number
        li   r1, '\n'
        out  r1
        addi r4, 1
        cmpi r4, LAST
        jle  next
        halt

; factorial: r0 becomes r0!, using r1; each call keeps its n on the stack.
factorial:
        cmpi r0, 1
        jle  base
        push r0
        addi r0, -1
        call factorial
        pop  r1
        mul  r0, r0, r1
        ret
base:   li   r0, 1
        ret

; print_number: writes r0 in decimal and keeps every register.
print_number:
        push r0
        push r1
        push r2
        li   r1, 10
        rem  r2, r0, r1
        div  r0, r0, r1
        cmpi r0, 0
        jeq  print_digit
        call print_number
print_digit:
        addi r2, '0'
        out  r2
        pop  r2
        pop  r1
        pop  r0
        ret
