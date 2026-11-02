; Sums 1..1000 in a loop and prints 500500.
LIMIT = 1000

        li   r0, 0              ; the total
        li   r1, 1              ; the next number
loop:   add  r0, r0, r1
        addi r1, 1
        cmpi r1, LIMIT
        jle  loop
        call print_number
        li   r1, '\n'
        out  r1
        halt

; print_number: writes r0 in decimal and keeps every register. The digits come out in order
; because each call prints the rest of the number first.
print_number:
        push r0
        push r1
        li   r1, 10
        rem  r2, r0, r1
        div  r0, r0, r1
        push r2
        cmpi r0, 0
        jeq  print_digit
        call print_number
print_digit:
        pop  r2
        addi r2, '0'
        out  r2
        pop  r1
        pop  r0
        ret
