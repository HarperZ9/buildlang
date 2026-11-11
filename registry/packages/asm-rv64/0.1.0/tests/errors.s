# A file with one error of each kind. `asm-rv64 asm` names every line and
# reason and prints no word. Lines 4, 22 and 26 are right.

start:  addi    x1, x0, 1
        addq    x1, x2, x3          # unknown mnemonic
        add     x1, x2, x32         # bad register
        addi    x1, x2, 2048        # immediate out of range
        beq     x1, x2, 3           # odd offset
        jal     x1, 1048576         # target out of range
        beq     x1, x2, nowhere     # undefined label
start:  ecall                       # duplicate label
        add     x1, x2              # wrong operand count
        add     x1, x2, x3 x4       # trailing text
        addi    x1, x2, 12a         # bad number
        lb      x1, x2              # expected imm(rs1)
        csrrw   x1, mfoo, x2        # unknown CSR name
        .org    2                   # .org not a multiple of 4
        .byte   1                   # unknown directive
        .org    0x10004             # image too large
1abc:   ecall                       # bad label name
        add     x1, , x3            # empty operand
        ecall
        slli    x1, x2, 64          # shift amount out of range
        sd      x1, -2049(x2)       # store offset out of range
        .word   0x100000000         # .word out of range
        mret
