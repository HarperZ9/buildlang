# Sum 1..10 in RV64IM
.text
.globl main
main:
    li a0, 0
    li t0, 1
    li t1, 11
loop:
    bge t0, t1, done
    add a0, a0, t0
    addi t0, t0, 1
    j loop
done:
    li a7, 1
    ecall
    ret
