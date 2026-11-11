# A program larger than the 65536 bytes of the small limits. The command
# `asm` refuses every statement from line 12 on. `asmx`, `bytesx`, `serial` and
# `serial-check`, which have the large limits, take it.
#
# It jumps over 65536 bytes, sends "far" and a line feed to the serial
# device and halts with exit code 3. The image is 65536 + 11 * 4 = 65580
# bytes. Steps, worked out by hand: jal, lui, four times addi and sb, and
# addi x10: 1 + 1 + 8 + 1 = 11 steps.

        jal     x0, far             # an offset of 65536 bytes
        .org    0x10000
far:    lui     x5, 0x10000         # x5 = 0x10000000, the serial device
        addi    x6, x0, 0x66        # "f"
        sb      x6, 0(x5)
        addi    x6, x0, 0x61        # "a"
        sb      x6, 0(x5)
        addi    x6, x0, 0x72        # "r"
        sb      x6, 0(x5)
        addi    x6, x0, 10          # line feed
        sb      x6, 0(x5)
        addi    x10, x0, 3          # exit code 3
        jal     x0, 0               # the halt word, 0x0000006F
