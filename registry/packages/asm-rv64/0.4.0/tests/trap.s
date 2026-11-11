# The trap-handler program of emu-rv64 (src/mprog.bld of 0.4.0), as text.
# It installs a handler, makes an environment call, and after the handler
# returns it sends the byte "!" to the serial device.
# The image is 84 bytes: main code at 0x00, zeros from 0x20, the handler
# at 0x40.

        auipc   x5, 0               # x5 = address of this instruction
        addi    x5, x5, 0x40        # x5 = the handler
        csrrw   x0, mtvec, x5
        ecall                       # trap, cause 11
        lui     x6, 0x10000         # x6 = 0x10000000, the serial device
        addi    x7, x0, 0x21        # the byte "!"
        sb      x7, 0(x6)
stay:   jal     x0, stay

        .org    0x40
        csrrs   x28, mepc, x0       # x28 = address of the ecall
        addi    x28, x28, 4
        csrrw   x0, mepc, x28       # return to the instruction after it
        csrrs   x29, mcause, x0     # x29 = 11
        mret
