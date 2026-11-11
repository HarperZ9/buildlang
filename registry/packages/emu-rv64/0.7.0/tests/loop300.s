# The image tests/loop300.img was made from this file with asm-rv64 0.4.0:
#     asm-rv64 bytesx tests/loop300.s | xxd -r -p > tests/loop300.img
# This package cannot assemble.
#
# The program of case 13 of `emu-rv64 zcases`, as text. It sends 300 bytes
# to the serial device: byte i, counted from 0, is the low 8 bits of i + 1.
# The output buffer of the run holds 128 bytes, so it is emptied twice.
# It halts with exit code 9.
#
# Steps, worked out by hand: 3 before the loop, 4 for each of the 300
# rounds, then addi x10: 3 + 300 * 4 + 1 = 1204 steps.

        lui     x5, 0x10000         # x5 = 0x10000000, the serial device
        addi    x6, x0, 300         # rounds left
        addi    x7, x0, 1           # the next byte
again:  sb      x7, 0(x5)
        addi    x7, x7, 1
        addi    x6, x6, -1
        bne     x6, x0, again
        addi    x10, x0, 9          # exit code 9
        jal     x0, 0               # the halt word, 0x0000006F
