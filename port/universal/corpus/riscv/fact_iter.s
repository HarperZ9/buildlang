# Factorial 7! in RV64IM
.text
.globl main
main:
    li a0, 1
    li t0, 7
    li t1, 1
fact_loop:
    beq t0, t1, fact_done
    mul a0, a0, t0
    addi t0, t0, -1
    j fact_loop
fact_done:
    li a7, 1
    ecall
    ret
