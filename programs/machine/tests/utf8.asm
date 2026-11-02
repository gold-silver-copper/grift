; Bytes above 127 make up UTF-8 characters: two-, three- and four-byte ones.
        la   r1, text
        la   r2, end
next:   cmp  r1, r2
        jae  done
        loadb r0, [r1]
        out  r0
        addi r1, 1
        jmp  next
done:   halt
text:   .string "é€𝄞\n"
end:
