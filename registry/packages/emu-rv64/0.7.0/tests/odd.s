# The image tests/odd.img was made from this file with asm-rv64 0.4.0:
#     asm-rv64 bytesx tests/odd.s | xxd -r -p > tests/odd.img
# This package cannot assemble.
#
# The program sends the five bytes 41 80 00 42 0A and halts with exit code
# 0. The command `serial` prints a byte from 1 to 127 as it is and any
# other byte as a question mark, so it prints A??B and a line feed, and
# its exit status is 125 because two bytes could not be printed.
#
# Steps, worked out by hand: lui, four times addi and sb less one addi
# (the zero byte is stored from x0), and addi x10: 1 + 9 + 1 = 11 steps.

        lui     x5, 0x10000         # x5 = 0x10000000, the serial device
        addi    x6, x0, 0x41        # "A"
        sb      x6, 0(x5)
        addi    x6, x0, 0x80        # a byte above 127
        sb      x6, 0(x5)
        sb      x0, 0(x5)           # a zero byte
        addi    x6, x0, 0x42        # "B"
        sb      x6, 0(x5)
        addi    x6, x0, 10          # line feed
        sb      x6, 0(x5)
        addi    x10, x0, 0          # exit code 0
        jal     x0, 0               # the halt word, 0x0000006F
