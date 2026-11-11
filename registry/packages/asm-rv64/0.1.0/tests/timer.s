# The timer program of emu-rv64 (src/iprog.bld of 0.4.0), as text.
# It installs a handler, sets mtimecmp to 100, enables the timer
# interrupt and loops. The handler moves mtimecmp on by 100, sends the
# byte "." to the serial device and counts in x12.
# The image is 88 bytes: main code at 0x00, zeros from 0x34, the handler
# at 0x40.

        lui     x5, 0x2004          # x5 = 0x02004000, mtimecmp
        addi    x6, x0, 100
        sd      x6, 0(x5)           # mtimecmp = 100
        lui     x13, 0x10000        # x13 = 0x10000000, the serial device
        addi    x14, x0, 0x2E       # the byte "."
        auipc   x7, 0               # x7 = address of this instruction, 0x14
        addi    x7, x7, 0x2C        # x7 = 0x40, the handler
        csrrw   x0, mtvec, x7       # direct mode
        addi    x8, x0, 0x80        # bit 7, the timer bit of mie
        csrrw   x0, mie, x8
        csrrsi  x0, mstatus, 8      # MIE = 1
loop:   addi    x10, x10, 1
        jal     x0, loop

        .org    0x40
handler:
        ld      x11, 0(x5)          # x11 = mtimecmp
        addi    x11, x11, 100
        sd      x11, 0(x5)          # mtimecmp moves on by 100
        sb      x14, 0(x13)         # sends "."
        addi    x12, x12, 1         # counts the interrupt
        mret
