# Raw memory store and privileged CSR must be refused
main:
    li a0, 42
    sd a0, 0(sp)
    csrrw t0, mstatus, a0
    ret
