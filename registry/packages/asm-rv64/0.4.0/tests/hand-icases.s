# The 50 words that `emu-rv64 icases` prints (src/ivec.bld to src/ivec3.bld of 0.4.0),
# as text, one statement per printed word, in the order of the golden file.
# `asm-rv64 hand` compares the words with the `word=` values of the golden file.

ld      x3, 0(x1)               # word 1
lw      x3, 4(x1)               # word 2
lw      x3, 0(x1)               # word 3
lwu     x3, 0(x1)               # word 4
lh      x3, 3(x1)               # word 5
lhu     x3, 6(x1)               # word 6
lb      x3, 7(x1)               # word 7
lbu     x3, 7(x1)               # word 8
ld      x3, 0(x1)               # word 9
lw      x3, 4(x1)               # word 10
ld      x3, 1(x1)               # word 11
lb      x3, -1(x1)              # word 12
lb      x3, 8(x1)               # word 13
sd      x2, 4(x1)               # word 14
sd      x2, 0(x1)               # word 15
sw      x2, 4(x1)               # word 16
sh      x2, 3(x1)               # word 17
sb      x2, 7(x1)               # word 18
sd      x2, 0(x1)               # word 19
sb      x2, 1(x1)               # word 20
csrrs   x3, mip, x0             # word 21
csrrs   x3, mip, x0             # word 22
csrrs   x3, mip, x0             # word 23
csrrs   x3, mip, x0             # word 24
csrrs   x3, mip, x0             # word 25
csrrs   x3, mip, x0             # word 26
csrrw   x3, mip, x1             # word 27
csrrw   x3, mip, x1             # word 28
addi    x3, x0, 1               # word 29
addi    x3, x0, 1               # word 30
addi    x3, x0, 1               # word 31
addi    x3, x0, 1               # word 32
addi    x3, x0, 1               # word 33
addi    x3, x0, 1               # word 34
addi    x3, x0, 1               # word 35
addi    x3, x0, 1               # word 36
addi    x3, x0, 1               # word 37
addi    x3, x0, 1               # word 38
addi    x3, x0, 1               # word 39
addi    x3, x0, 1               # word 40
addi    x3, x0, 1               # word 41
addi    x3, x0, 1               # word 42
addi    x3, x0, 1               # word 43
addi    x3, x0, 1               # word 44
ecall                           # word 45
addi    x3, x0, 1               # word 46
wfi                             # word 47
wfi                             # word 48
wfi                             # word 49
addi    x3, x0, 1               # word 50
