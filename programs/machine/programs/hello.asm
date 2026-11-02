; Writes a greeting, one byte at a time.
start:  la   r1, message
        la   r2, message_end
loop:   cmp  r1, r2
        jae  done
        loadb r0, [r1]
        out  r0
        addi r1, 1
        jmp  loop
done:   halt

message:
        .string "hello, world\n"
message_end:
