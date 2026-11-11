# The 59 hand-computed cases of `emu-rv64 mcases` (src/mvec.bld and src/mvec2.bld of
# 0.4.0), as text, one statement per case, in case order.
# `asm-rv64 hand` compares the words with the `word=` values of the golden file.

csrrw   x3, mscratch, x1        # mcase 1
csrrw   x0, mscratch, x1        # mcase 2
csrrw   x1, mscratch, x1        # mcase 3
csrrw   x3, mstatus, x1         # mcase 4
csrrw   x3, mstatus, x1         # mcase 5
csrrw   x3, mepc, x1            # mcase 6
csrrw   x3, mtvec, x1           # mcase 7
csrrw   x3, mip, x1             # mcase 8
csrrw   x3, misa, x1            # mcase 9
csrrw   x3, mcause, x1          # mcase 10
csrrw   x3, mtval, x1           # mcase 11
csrrw   x3, mie, x1             # mcase 12
csrrw   x0, mhartid, x1         # mcase 13
csrrs   x3, mie, x1             # mcase 14
csrrs   x3, mscratch, x0        # mcase 15
csrrs   x3, mhartid, x0         # mcase 16
csrrs   x3, misa, x0            # mcase 17
csrrs   x3, mhartid, x1         # mcase 18
csrrs   x3, 0x7c0, x0           # mcase 19
csrrs   x3, 0xc00, x0           # mcase 20
csrrs   x3, mscratch, x1        # mcase 21
csrrs   x3, mstatus, x0         # mcase 22
csrrc   x3, mscratch, x1        # mcase 23
csrrc   x3, mscratch, x0        # mcase 24
csrrc   x3, mstatus, x1         # mcase 25
csrrc   x3, mhartid, x0         # mcase 26
csrrwi  x3, mscratch, 21        # mcase 27
csrrwi  x3, mscratch, 0         # mcase 28
csrrwi  x0, mscratch, 31        # mcase 29
csrrsi  x3, mie, 8              # mcase 30
csrrsi  x3, mhartid, 0          # mcase 31
csrrci  x3, mstatus, 8          # mcase 32
csrrci  x3, mstatus, 0          # mcase 33
csrrci  x3, mhartid, 1          # mcase 34
csrrwi  x0, mhartid, 0          # mcase 35
ecall                           # mcase 36
ebreak                          # mcase 37
.word   0x00000000              # mcase 38
.word   0xFFFFFFFF              # mcase 39
ld      x3, 0(x1)               # mcase 40
lb      x3, 0(x1)               # mcase 41
sd      x2, 8(x1)               # mcase 42
sd      x2, 8(x1)               # mcase 43
sh      x2, 8(x1)               # mcase 44
jalr    x3, 5(x1)               # mcase 45
beq     x1, x2, 6               # mcase 46
beq     x1, x2, 6               # mcase 47
addi    x0, x0, 0               # mcase 48
addi    x0, x0, 0               # mcase 49
ecall                           # mcase 50
ecall                           # mcase 51
ecall                           # mcase 52
mret                            # mcase 53
mret                            # mcase 54
mret                            # mcase 55
mret                            # mcase 56
csrrw   x3, mscratch, x1        # mcase 57
mret                            # mcase 58
wfi                             # mcase 59
