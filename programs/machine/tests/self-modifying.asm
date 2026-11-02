; Memory is code: this overwrites a nop with a copy of an out instruction and then runs it.
        li   r0, '!'
        la   r1, template
        load r2, [r1]
        la   r1, patch
        store r2, [r1]
patch:  nop
        li   r0, '\n'
        out  r0
        halt
template:
        out  r0
