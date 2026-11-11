# Every mnemonic of asm-rv64 once, in the order of the operation numbers
# of emu-rv64, then labels, .word and .org. `asm-rv64 asm` gives one word
# per statement, and `asm-rv64 dis` of those words gives text that
# assembles to the same words.

        lui     x1, 0xFFFFF
        auipc   x2, 0
first:  jal     x1, last            # forward, over the whole file
        jalr    x0, -2048(x1)
        beq     x1, x2, first       # backward
        bne     x3, x4, -4096       # the lowest branch offset
        blt     x5, x6, 4094        # the highest
        bge     x7, x8, first
        bltu    x9, x10, last
        bgeu    x11, x12, 0
        lb      x13, -2048(x14)
        lh      x15, 2047(x16)
        lw      x17, -1(x18)
        ld      x19, 0(x20)
        lbu     x21, 1(x22)
        lhu     x23, 0x7ff(x24)
        lwu     x25, -0x800(x26)
        sb      x27, -2048(x28)
        sh      x29, 2047(x30)
        sw      x31, -1(x0)
        sd      x2, 8(x1)
        addi    x1, x2, -2048
        slti    x3, x4, 2047
        sltiu   x5, x6, -1
        xori    x7, x8, 0
        ori     x9, x10, 0x555
        andi    x11, x12, -0x556
        slli    x13, x14, 63
        srli    x15, x16, 0
        srai    x17, x18, 63
        add     x19, x20, x21
        sub     x22, x23, x24
        sll     x25, x26, x27
        slt     x28, x29, x30
        sltu    x31, x0, x1
        xor     x2, x3, x4
        srl     x5, x6, x7
        sra     x8, x9, x10
        or      x11, x12, x13
        and     x14, x15, x16
        addiw   x17, x18, -1
        slliw   x19, x20, 31
        srliw   x21, x22, 0
        sraiw   x23, x24, 31
        addw    x25, x26, x27
        subw    x28, x29, x30
        sllw    x31, x0, x1
        srlw    x2, x3, x4
        sraw    x5, x6, x7
        fence   x0, x0, 0x0ff
        ecall
        ebreak
        csrrw   x1, mstatus, x2
        csrrs   x3, misa, x0
        csrrc   x4, mie, x5
        csrrwi  x6, mtvec, 31
        csrrsi  x7, mscratch, 0
        csrrci  x8, mepc, 1
        csrrs   x9, mcause, x0
        csrrs   x10, mtval, x0
        csrrs   x11, mip, x0
        csrrs   x12, mvendorid, x0
        csrrs   x13, marchid, x0
        csrrs   x14, mimpid, x0
        csrrs   x15, mhartid, x0
        csrrs   x16, mconfigptr, x0
        csrrw   x17, 0x7c0, x18     # a CSR with no name here
        csrrw   x19, 4095, x20
        mret
        wfi
        .word   0xDEADBEEF
        .word   0
        .org    0x130               # four zero words
last:   jal     x0, first
