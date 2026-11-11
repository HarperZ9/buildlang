# The 61 hand-computed cases of `emu-rv64 cases` (src/vectors.bld of 0.4.0), as text,
# one statement per case, in case order. Three cases use a word that is no
# instruction; they are written with .word.
# `asm-rv64 hand` compares the words with the `word=` values of the golden file.

add     x3, x1, x2              # case 1
sub     x3, x1, x2              # case 2
sll     x3, x1, x2              # case 3
slt     x3, x1, x2              # case 4
sltu    x3, x1, x2              # case 5
xor     x3, x1, x2              # case 6
srl     x3, x1, x2              # case 7
sra     x3, x1, x2              # case 8
or      x3, x1, x2              # case 9
and     x3, x1, x2              # case 10
addi    x3, x1, -1              # case 11
slti    x3, x1, -1              # case 12
sltiu   x3, x1, -1              # case 13
xori    x3, x1, -1              # case 14
ori     x3, x1, 15              # case 15
andi    x3, x1, 15              # case 16
slli    x3, x1, 63              # case 17
srli    x3, x1, 63              # case 18
srai    x3, x1, 63              # case 19
lui     x3, 0x80000             # case 20
auipc   x3, 0x1                 # case 21
addi    x0, x1, 5               # case 22
jal     x3, 8192                # case 23
jalr    x3, 5(x1)               # case 24
jalr    x3, 5(x1)               # case 25
beq     x1, x2, -8              # case 26
bne     x1, x2, -8              # case 27
blt     x1, x2, -8              # case 28
bge     x1, x2, -8              # case 29
bltu    x1, x2, -8              # case 30
bgeu    x1, x2, -8              # case 31
addiw   x3, x1, 0               # case 32
addw    x3, x1, x2              # case 33
subw    x3, x1, x2              # case 34
sllw    x3, x1, x2              # case 35
srlw    x3, x1, x2              # case 36
sraw    x3, x1, x2              # case 37
slliw   x3, x1, 31              # case 38
srliw   x3, x1, 31              # case 39
sraiw   x3, x1, 31              # case 40
lb      x3, 0(x1)               # case 41
lh      x3, 0(x1)               # case 42
lw      x3, 0(x1)               # case 43
ld      x3, 0(x1)               # case 44
lbu     x3, 0(x1)               # case 45
lhu     x3, 0(x1)               # case 46
lwu     x3, 0(x1)               # case 47
sb      x2, 8(x1)               # case 48
sh      x2, 8(x1)               # case 49
sw      x2, 8(x1)               # case 50
sd      x2, 8(x1)               # case 51
lb      x3, 0(x1)               # case 52
ld      x3, 0(x1)               # case 53
lbu     x3, 5(x1)               # case 54
sd      x2, 8(x1)               # case 55
ecall                           # case 56
ebreak                          # case 57
.word   0x00000000              # case 58
.word   0xFFFFFFFF              # case 59
fence   x0, x0, 0x0ff           # case 60
.word   0x04009193              # case 61
