; Fills memory with 100 numbers from a 16-bit linear congruential generator, prints them, sorts them
; in place by insertion and prints them again.
COUNT = 100
SIZE = COUNT * 4

        call fill
        call print_table
        call sort
        call print_table
        halt

; fill: table[i] = x % 1000, with x = (x * 25173 + 13849) % 65536.
fill:   la   r1, table
        la   r2, table + SIZE
        li   r3, 1              ; x
        li   r4, 25173
        la   r5, 65535
        li   r6, 1000
fill_next:
        mul  r3, r3, r4
        addi r3, 13849
        and  r3, r3, r5
        rem  r0, r3, r6
        store r0, [r1]
        addi r1, 4
        cmp  r1, r2
        jb   fill_next
        ret

; sort: insertion sort of the table.
sort:   la   r1, table + 4      ; the next one to insert
        la   r2, table + SIZE
sort_next:
        cmp  r1, r2
        jae  sort_done
        load r3, [r1]           ; its value
        mov  r4, r1             ; where it may go
shift:  la   r5, table
        cmp  r4, r5
        jeq  place
        load r5, [r4-4]
        cmp  r5, r3
        jle  place
        store r5, [r4]
        addi r4, -4
        jmp  shift
place:  store r3, [r4]
        addi r1, 4
        jmp  sort_next
sort_done:
        ret

; print_table: the numbers on one line.
print_table:
        la   r1, table
        la   r2, table + SIZE
print_next:
        load r0, [r1]
        call print_number
        li   r0, ' '
        out  r0
        addi r1, 4
        cmp  r1, r2
        jb   print_next
        li   r0, '\n'
        out  r0
        ret

; print_number: writes r0 in decimal and keeps every register but r0.
print_number:
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
        ret

table:  .space SIZE
