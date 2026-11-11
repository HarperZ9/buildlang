# mulatomic.s: multiply, divide and an LR/SC retry loop, for emu-rv64 0.5.0.
#
# The program works out 12 * -7 = -84, divides it by 5, and then adds the
# quotient three times to a doubleword in memory with LR and SC. It enters
# the loop after the LR, so the first SC has no reservation and fails, and
# the loop retries. At the end it checks two results itself: on success it
# sends the byte 'P' to the serial device and stays at `pass`.
#
# Expected values, worked out by hand:
#   x12 = 12 * -7              = -84   (0xFFFFFFFFFFFFFFAC)
#   x14 = -84 / 5              = -16   (rounds toward zero), then negated: 16
#   x15 = -84 rem 5            = -4    (sign of the dividend), then negated: 4
#   x7  = upper half of -84 * -84 = 7056: 0
#   x8  = 5 / 0 unsigned       = all ones
#   The loop: step 15 is the first `add`, with x16 = 0, so x17 = 16; step 16
#   is an SC with no reservation: x18 = 1, nothing stored, x26 = 1. Then
#   three rounds of LR, add, SC: cell = 1000 -> 1016 -> 1032 -> 1048, with
#   x19 = 3. At the end x16 = 1032 (the last value LR read), x17 = 1048 and
#   x18 = 0.
#   x21 = 1048, the value AMOADD read; cell = 1048 + 4 = 1052; x22 = 1052.
#   x26 = 1, the number of SC instructions that failed.
# Steps: 14 before the loop, 4 for the failed attempt, 8 for each of the
# three rounds, 8 after the loop: step 50 is the `sb`, and from step 51 on
# the machine runs `jal x0, pass`. After 100 steps: 100 instructions
# retired, no trap, pc = 0x80000078, one byte sent.

        auipc   x5, 0               # 00: x5 = 0x80000000
        addi    x5, x5, 0xC0        # 04: the address of `cell`
        lui     x6, 0x10000         # 08: the serial device
        addi    x10, x0, 12         # 0C
        addi    x11, x0, -7         # 10
        mul     x12, x10, x11       # 14: -84
        addi    x13, x0, 5          # 18
        div     x14, x12, x13       # 1C: -16
        rem     x15, x12, x13       # 20: -4
        mulh    x7, x12, x12        # 24: 0
        divu    x8, x13, x0         # 28: all ones
        sub     x14, x0, x14        # 2C: 16
        sub     x15, x0, x15        # 30: 4
        jal     x0, try             # 34: enter the loop past the LR
retry:  lr.d    x16, (x5)           # 38
try:    add     x17, x16, x14       # 3C
        sc.d    x18, x17, (x5)      # 40: 0 when it stored
        add     x26, x26, x18       # 44: count the failures
        bne     x18, x0, retry      # 48
        addi    x19, x19, 1         # 4C
        addi    x20, x0, 3          # 50
        bne     x19, x20, retry     # 54
        amoadd.d x21, x15, (x5)     # 58: x21 = 1048, cell = 1052
        ld      x22, 0(x5)          # 5C
        addi    x27, x0, 1052       # 60
        bne     x22, x27, fail      # 64
        addi    x28, x0, 1          # 68
        bne     x26, x28, fail      # 6C
        addi    x29, x0, 80         # 70: the letter P
        sb      x29, 0(x6)          # 74
pass:   jal     x0, pass            # 78
fail:   jal     x0, fail            # 7C
        .org    0xC0
cell:   .word   1000                # the low half of the doubleword
        .word   0
