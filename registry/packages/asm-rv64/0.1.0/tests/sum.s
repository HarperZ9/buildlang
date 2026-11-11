# The sum program of `emu-rv64 cases` (program_sum in src/main.bld of
# 0.4.0), as text. It adds 10 + 9 + ... + 1 in x1 and stops on EBREAK.

        addi    x1, x0, 0           # the sum
        addi    x2, x0, 10          # the counter
again:  add     x1, x1, x2
        addi    x2, x2, -1
        bne     x2, x0, again
        ebreak
