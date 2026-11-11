    jal x0, rt_start
    jal x0, rt_mtrap
rt_start:
    addi x5, x0, 1
    slli x5, x5, 31
    addi x6, x5, 4
    csrrw x0, mtvec, x6
    lui x6, 0x400
    add x2, x5, x6
    lui x6, 0x100
    add x26, x5, x6
    addi x27, x26, 1024
    addi x10, x0, 0
    addi x11, x0, 70
    addi x12, x0, 5
    addi x13, x0, 400
    jal x1, fn_0
    addi x10, x0, 0
    jal x0, rt_halt
rt_die:
    addi x9, x11, 0
    jal x1, rt_puts
    addi x10, x9, 0
rt_halt:
    jal x0, 0
rt_puts:
    lui x5, 0x10000
rt_puts_next:
    lbu x6, 0(x10)
    beq x6, x0, rt_puts_end
    sb x6, 0(x5)
    addi x10, x10, 1
    jal x0, rt_puts_next
rt_puts_end:
    jalr x0, 0(x1)
rt_put_i64:
    bge x10, x0, rt_put_u64
    lui x5, 0x10000
    addi x6, x0, 45
    sb x6, 0(x5)
    sub x10, x0, x10
rt_put_u64:
    addi x2, x2, -32
    addi x5, x2, 31
    sb x0, 0(x5)
    addi x6, x0, 10
rt_put_u64_digit:
    remu x7, x10, x6
    divu x10, x10, x6
    addi x7, x7, 48
    addi x5, x5, -1
    sb x7, 0(x5)
    bne x10, x0, rt_put_u64_digit
    lui x6, 0x10000
rt_put_u64_out:
    lbu x7, 0(x5)
    beq x7, x0, rt_put_u64_end
    sb x7, 0(x6)
    addi x5, x5, 1
    jal x0, rt_put_u64_out
rt_put_u64_end:
    addi x2, x2, 32
    jalr x0, 0(x1)
rt_put_hex:
    addi x5, x0, 1
    srli x6, x10, 4
rt_put_hex_count:
    beq x6, x0, rt_put_hex_pad
    addi x5, x5, 1
    srli x6, x6, 4
    jal x0, rt_put_hex_count
rt_put_hex_pad:
    lui x7, 0x10000
rt_put_hex_fill:
    bge x5, x11, rt_put_hex_digit
    sb x12, 0(x7)
    addi x11, x11, -1
    jal x0, rt_put_hex_fill
rt_put_hex_digit:
    addi x5, x5, -1
    slli x6, x5, 2
    srl x6, x10, x6
    andi x6, x6, 15
    addi x28, x0, 10
    blt x6, x28, rt_put_hex_low
    addi x6, x6, 39
rt_put_hex_low:
    addi x6, x6, 48
    sb x6, 0(x7)
    bne x5, x0, rt_put_hex_digit
    jalr x0, 0(x1)
rt_ovf_add:
    jal x10, 36
    .word 1702130785
    .word 544501869
    .word 1629515636
    .word 1998611556
    .word 543716457
    .word 1919252079
    .word 2003790950
    .word 10
    addi x11, x0, 101
    jal x0, rt_die
rt_ovf_sub:
    jal x10, 40
    .word 1702130785
    .word 544501869
    .word 1931505524
    .word 1920230005
    .word 544498529
    .word 1752459639
    .word 1702260512
    .word 1869375090
    .word 2679
    addi x11, x0, 101
    jal x0, rt_die
rt_ovf_mul:
    jal x10, 40
    .word 1702130785
    .word 544501869
    .word 1830842228
    .word 1769237621
    .word 544828528
    .word 1752459639
    .word 1702260512
    .word 1869375090
    .word 2679
    addi x11, x0, 101
    jal x0, rt_die
rt_ovf_neg:
    jal x10, 40
    .word 1702130785
    .word 544501869
    .word 1847619444
    .word 1952540517
    .word 1769414757
    .word 1864394868
    .word 1718773110
    .word 175599468
    .word 0
    addi x11, x0, 101
    jal x0, rt_die
rt_div_zero:
    jal x10, 32
    .word 1702130785
    .word 544501869
    .word 1679847284
    .word 1684633193
    .word 2036473957
    .word 1919253024
    .word 2671
    addi x11, x0, 101
    jal x0, rt_die
rt_ovf_div:
    jal x10, 40
    .word 1702130785
    .word 544501869
    .word 1679847284
    .word 1684633193
    .word 1769414757
    .word 1864394868
    .word 1718773110
    .word 175599468
    .word 0
    addi x11, x0, 101
    jal x0, rt_die
rt_rem_zero:
    jal x10, 64
    .word 1702130785
    .word 544501869
    .word 1663070068
    .word 1969450081
    .word 1702125932
    .word 1701344288
    .word 1835364896
    .word 1684957537
    .word 1998615141
    .word 543716457
    .word 1768169569
    .word 1869834614
    .word 1718558834
    .word 1919253024
    .word 2671
    addi x11, x0, 101
    jal x0, rt_die
rt_ovf_rem:
    jal x10, 56
    .word 1702130785
    .word 544501869
    .word 1663070068
    .word 1969450081
    .word 1702125932
    .word 1701344288
    .word 1835364896
    .word 1684957537
    .word 1998615141
    .word 543716457
    .word 1919252079
    .word 2003790950
    .word 10
    addi x11, x0, 101
    jal x0, rt_die
rt_stack:
    jal x10, 20
    .word 1667331187
    .word 1986994283
    .word 1818653285
    .word 685935
    addi x11, x0, 103
    jal x0, rt_die
rt_memory:
    jal x10, 20
    .word 544503151
    .word 1830839919
    .word 1919905125
    .word 2681
    addi x11, x0, 104
    jal x0, rt_die
rt_oob:
    addi x8, x10, 0
    addi x9, x11, 0
    jal x10, 40
    .word 1701080681
    .word 1970217080
    .word 1718558836
    .word 1970233888
    .word 980640878
    .word 1701344288
    .word 1852140576
    .word 544434464
    .word 0
    jal x1, rt_puts
    addi x10, x8, 0
    jal x1, rt_put_u64
    jal x10, 24
    .word 1953849888
    .word 1701344288
    .word 1684957472
    .word 1763735653
    .word 8307
    jal x1, rt_puts
    addi x10, x9, 0
    jal x1, rt_put_i64
    jal x0, rt_oob_end
rt_vec_oob:
    addi x8, x10, 0
    addi x9, x11, 0
    jal x10, 32
    .word 543384918
    .word 1701080681
    .word 1970217080
    .word 1718558836
    .word 1970233888
    .word 980640878
    .word 32
    jal x1, rt_puts
    addi x10, x8, 0
    jal x1, rt_put_u64
    jal x10, 12
    .word 540884512
    .word 0
    jal x1, rt_puts
    addi x10, x9, 0
    jal x1, rt_put_u64
rt_oob_end:
    jal x10, 8
    .word 10
    addi x11, x0, 101
    jal x0, rt_die
rt_mtrap:
    jal x10, 28
    .word 1751343469
    .word 543518313
    .word 1885434484
    .word 1668096058
    .word 1702065505
    .word 61
    jal x1, rt_puts
    csrrs x10, mcause, x0
    jal x1, rt_mtrap_hex
    jal x10, 12
    .word 1885695264
    .word 15715
    jal x1, rt_puts
    csrrs x10, mepc, x0
    jal x1, rt_mtrap_hex
    jal x10, 12
    .word 1987341600
    .word 4025441
    jal x1, rt_puts
    csrrs x10, mtval, x0
    jal x1, rt_mtrap_hex
    jal x10, 8
    .word 10
    addi x11, x0, 102
    jal x0, rt_die
rt_mtrap_hex:
    addi x11, x0, 1
    addi x12, x0, 48
    jal x0, rt_put_hex
rt_alloc:
    addi x10, x10, 15
    andi x10, x10, -16
    add x5, x26, x10
    bltu x5, x26, rt_alloc_fail
    addi x6, x5, 1024
    bltu x6, x5, rt_alloc_fail
    bltu x2, x6, rt_alloc_fail
    addi x10, x26, 0
    addi x26, x5, 0
    addi x27, x6, 0
    jalr x0, 0(x1)
rt_alloc_fail:
    jal x0, rt_memory
rt_vec_new:
    addi x2, x2, -16
    sd x1, 8(x2)
    addi x10, x0, 24
    jal x1, rt_alloc
    sd x0, 0(x10)
    sd x0, 8(x10)
    sd x0, 16(x10)
    ld x1, 8(x2)
    addi x2, x2, 16
    jalr x0, 0(x1)
rt_vec_push:
    ld x5, 8(x10)
    ld x6, 16(x10)
    bltu x5, x6, rt_vec_push_store
    addi x2, x2, -48
    sd x1, 40(x2)
    sd x10, 32(x2)
    sd x11, 24(x2)
    sd x12, 16(x2)
    slli x7, x6, 1
    bne x6, x0, rt_vec_push_grow
    addi x7, x0, 8
rt_vec_push_grow:
    sd x7, 8(x2)
    mul x10, x7, x12
    jal x1, rt_alloc
    ld x5, 32(x2)
    ld x6, 0(x5)
    ld x7, 8(x5)
    ld x12, 16(x2)
    mul x7, x7, x12
    addi x28, x10, 0
rt_vec_push_copy:
    beq x7, x0, rt_vec_push_moved
    lbu x29, 0(x6)
    sb x29, 0(x28)
    addi x6, x6, 1
    addi x28, x28, 1
    addi x7, x7, -1
    jal x0, rt_vec_push_copy
rt_vec_push_moved:
    sd x10, 0(x5)
    ld x7, 8(x2)
    sd x7, 16(x5)
    addi x10, x5, 0
    ld x11, 24(x2)
    ld x1, 40(x2)
    addi x2, x2, 48
    ld x5, 8(x10)
rt_vec_push_store:
    ld x6, 0(x10)
    mul x7, x5, x12
    add x6, x6, x7
    addi x7, x0, 1
    beq x12, x7, rt_vec_push_1
    addi x7, x0, 2
    beq x12, x7, rt_vec_push_2
    addi x7, x0, 4
    beq x12, x7, rt_vec_push_4
    sd x11, 0(x6)
    jal x0, rt_vec_push_end
rt_vec_push_1:
    sb x11, 0(x6)
    jal x0, rt_vec_push_end
rt_vec_push_2:
    sh x11, 0(x6)
    jal x0, rt_vec_push_end
rt_vec_push_4:
    sw x11, 0(x6)
rt_vec_push_end:
    addi x5, x5, 1
    sd x5, 8(x10)
    jalr x0, 0(x1)
# trace
fn_0:
    addi x2, x2, -624
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 616(x2)
    sb x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sd x13, 24(x2)
f0_0:
    addi x10, x2, 32
    jal x1, fn_1
    jal x0, f0_1
f0_1:
    addi x6, x2, 56
    addi x7, x2, 32
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    addi x5, x0, 0
    sd x5, 80(x2)
    addi x5, x0, 0
    sd x5, 88(x2)
    addi x5, x0, 0
    sd x5, 96(x2)
    addi x5, x0, 0
    sd x5, 104(x2)
    addi x5, x0, 0
    sd x5, 112(x2)
    addi x5, x0, 0
    sd x5, 120(x2)
    addi x5, x0, 0
    sd x5, 128(x2)
    addi x5, x0, 0
    sd x5, 136(x2)
    addi x6, x2, 144
    addi x7, x2, 80
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x28, 24(x7)
    sd x28, 24(x6)
    ld x28, 32(x7)
    sd x28, 32(x6)
    ld x28, 40(x7)
    sd x28, 40(x6)
    ld x28, 48(x7)
    sd x28, 48(x6)
    ld x28, 56(x7)
    sd x28, 56(x6)
    ld x10, 8(x2)
    jal x1, fn_2
    sd x10, 208(x2)
    jal x0, f0_2
f0_2:
    ld x5, 208(x2)
    sd x5, 216(x2)
    ld x5, 16(x2)
    sd x5, 224(x2)
    addi x6, x2, 232
    addi x7, x2, 224
    ld x28, 0(x7)
    sd x28, 0(x6)
    addi x7, x2, 56
    addi x5, x7, 0
    sd x5, 240(x2)
    addi x5, x2, 144
    sd x5, 248(x2)
    addi x5, x0, 8
    sd x5, 256(x2)
    addi x7, x2, 248
    addi x5, x7, 0
    sd x5, 264(x2)
    ld x10, 240(x2)
    ld x11, 264(x2)
    ld x12, 8(x2)
    jal x1, fn_3
    sd x10, 272(x2)
    jal x0, f0_3
f0_3:
    addi x5, x0, 1
    sd x5, 280(x2)
    jal x0, f0_4
f0_4:
    ld x5, 280(x2)
    ld x6, 24(x2)
    sltu x5, x6, x5
    xori x5, x5, 1
    sb x5, 288(x2)
    lbu x5, 288(x2)
    beq x5, x0, 8
    jal x0, f0_5
    jal x0, f0_6
f0_5:
    addi x7, x2, 232
    addi x5, x7, 0
    sd x5, 296(x2)
    ld x11, 296(x2)
    ld x12, 8(x2)
    ld x13, 280(x2)
    addi x10, x2, 304
    jal x1, fn_4
    jal x0, f0_7
f0_6:
    jal x5, 12
    .word 2216829733
    .word 3421674724
    ld x5, 0(x5)
    sd x5, 448(x2)
    addi x5, x0, 0
    sd x5, 456(x2)
    jal x0, f0_15
f0_7:
    addi x6, x2, 328
    addi x7, x2, 304
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    lbu x5, 0(x2)
    beq x5, x0, 8
    jal x0, f0_8
    jal x0, f0_9
f0_8:
    addi x7, x2, 328
    addi x5, x7, 0
    sd x5, 352(x2)
    addi x7, x2, 216
    addi x5, x7, 0
    sd x5, 360(x2)
    addi x7, x2, 328
    addi x5, x7, 0
    sd x5, 368(x2)
    ld x10, 360(x2)
    ld x11, 368(x2)
    jal x1, fn_5
    sd x10, 376(x2)
    jal x0, f0_11
f0_9:
    addi x7, x2, 328
    addi x5, x7, 0
    sd x5, 384(x2)
    addi x7, x2, 56
    addi x5, x7, 0
    sd x5, 392(x2)
    addi x5, x2, 144
    sd x5, 400(x2)
    addi x5, x0, 8
    sd x5, 408(x2)
    addi x7, x2, 400
    addi x5, x7, 0
    sd x5, 416(x2)
    addi x7, x2, 328
    addi x5, x7, 0
    sd x5, 424(x2)
    addi x10, x0, 0
    ld x11, 392(x2)
    ld x12, 416(x2)
    ld x13, 424(x2)
    jal x1, fn_6
    sd x10, 432(x2)
    jal x0, f0_13
f0_10:
    ld x5, 280(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 440(x2)
    ld x5, 440(x2)
    sd x5, 280(x2)
    jal x0, f0_4
f0_11:
    ld x10, 352(x2)
    ld x11, 376(x2)
    jal x1, fn_7
    jal x0, f0_12
f0_12:
    jal x0, f0_10
f0_13:
    ld x10, 384(x2)
    ld x11, 432(x2)
    jal x1, fn_7
    jal x0, f0_14
f0_14:
    jal x0, f0_10
f0_15:
    ld x5, 456(x2)
    ld x6, 8(x2)
    sltu x5, x5, x6
    sb x5, 464(x2)
    lbu x5, 464(x2)
    beq x5, x0, 8
    jal x0, f0_16
    jal x0, f0_17
f0_16:
    lbu x5, 0(x2)
    beq x5, x0, 8
    jal x0, f0_18
    jal x0, f0_19
f0_17:
    addi x7, x2, 64
    ld x5, 0(x7)
    sd x5, 560(x2)
    ld x5, 560(x2)
    sd x5, 568(x2)
    lbu x5, 0(x2)
    beq x5, x0, 8
    jal x0, f0_24
    jal x0, f0_25
f0_18:
    ld x5, 456(x2)
    sd x5, 472(x2)
    ld x28, 472(x2)
    ld x6, 216(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    add x7, x6, x28
    lbu x5, 0(x7)
    sb x5, 480(x2)
    lbu x5, 480(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 488(x2)
    ld x10, 448(x2)
    lbu x11, 488(x2)
    jal x1, fn_8
    sd x10, 496(x2)
    jal x0, f0_21
f0_19:
    addi x7, x2, 56
    addi x5, x7, 0
    sd x5, 504(x2)
    addi x5, x2, 144
    sd x5, 512(x2)
    addi x5, x0, 8
    sd x5, 520(x2)
    addi x7, x2, 512
    addi x5, x7, 0
    sd x5, 528(x2)
    ld x10, 504(x2)
    ld x11, 528(x2)
    ld x12, 456(x2)
    jal x1, fn_9
    sb x10, 536(x2)
    jal x0, f0_22
f0_20:
    ld x5, 456(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 552(x2)
    ld x5, 552(x2)
    sd x5, 456(x2)
    jal x0, f0_15
f0_21:
    ld x5, 496(x2)
    sd x5, 448(x2)
    jal x0, f0_20
f0_22:
    ld x10, 448(x2)
    lbu x11, 536(x2)
    jal x1, fn_8
    sd x10, 544(x2)
    jal x0, f0_23
f0_23:
    ld x5, 544(x2)
    sd x5, 448(x2)
    jal x0, f0_20
f0_24:
    addi x7, x2, 216
    addi x5, x7, 0
    sd x5, 576(x2)
    ld x10, 576(x2)
    jal x1, fn_10
    sd x10, 584(x2)
    jal x0, f0_27
f0_25:
    jal x0, f0_26
f0_26:
    jal x5, 44
    .word 1835102822
    .word 624784229
    .word 544566380
    .word 1701147238
    .word 1819026749
    .word 1768169589
    .word 1953719655
    .word 825238845
    .word 2020371510
    .word 10
    sd x5, 592(x2)
    jal x10, 12
    .word 1835102822
    .word 4027237
    jal x1, rt_puts
    ld x10, 8(x2)
    jal x1, rt_put_u64
    jal x10, 12
    .word 1701996064
    .word 15717
    jal x1, rt_puts
    ld x10, 568(x2)
    jal x1, rt_put_u64
    jal x10, 16
    .word 1734960160
    .word 1031041893
    .word 0
    jal x1, rt_puts
    ld x10, 448(x2)
    addi x11, x0, 16
    addi x12, x0, 48
    jal x1, rt_put_hex
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_28
f0_27:
    ld x5, 584(x2)
    sd x5, 568(x2)
    jal x0, f0_26
f0_28:
    ld x1, 616(x2)
    addi x2, x2, 624
    jalr x0, 0(x1)
# pfa_new
fn_1:
    addi x2, x2, -64
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 56(x2)
    sd x10, 48(x2)
f1_0:
    addi x5, x0, 0
    sd x5, 24(x2)
    addi x5, x0, 0
    sd x5, 32(x2)
    addi x5, x0, 0
    sd x5, 40(x2)
    ld x6, 48(x2)
    addi x7, x2, 24
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 56(x2)
    addi x2, x2, 64
    jalr x0, 0(x1)
# ref_new
fn_2:
    addi x2, x2, -80
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 72(x2)
    sd x10, 0(x2)
f2_0:
    jal x1, rt_vec_new
    sd x10, 16(x2)
    jal x0, f2_1
f2_1:
    ld x5, 16(x2)
    sd x5, 24(x2)
    addi x5, x0, 0
    sd x5, 32(x2)
    jal x0, f2_2
f2_2:
    ld x5, 32(x2)
    ld x6, 0(x2)
    sltu x5, x5, x6
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f2_3
    jal x0, f2_4
f2_3:
    ld x10, 24(x2)
    addi x11, x0, 0
    addi x12, x0, 1
    jal x1, rt_vec_push
    jal x0, f2_5
f2_4:
    ld x10, 24(x2)
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f2_5:
    ld x5, 32(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 48(x2)
    ld x5, 48(x2)
    sd x5, 32(x2)
    jal x0, f2_2
# pfa_init
fn_3:
    addi x2, x2, -208
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 200(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f3_0:
    ld x5, 16(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 32(x2)
    lbu x5, 32(x2)
    beq x5, x0, 8
    jal x0, f3_2
    jal x0, f3_1
f3_1:
    ld x5, 16(x2)
    jal x6, 12
    .word 0
    .word 256
    ld x6, 0(x6)
    sltu x5, x6, x5
    sb x5, 48(x2)
    lbu x5, 48(x2)
    sb x5, 40(x2)
    jal x0, f3_3
f3_2:
    addi x5, x0, 1
    sb x5, 40(x2)
    jal x0, f3_3
f3_3:
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f3_4
    jal x0, f3_5
f3_4:
    addi x10, x0, -2
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f3_5:
    jal x0, f3_6
f3_6:
    ld x10, 16(x2)
    jal x1, fn_11
    sd x10, 56(x2)
    jal x0, f3_8
f3_8:
    ld x5, 56(x2)
    sd x5, 64(x2)
    ld x7, 8(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 72(x2)
    ld x5, 72(x2)
    sd x5, 80(x2)
    ld x5, 80(x2)
    ld x6, 64(x2)
    sltu x5, x5, x6
    sb x5, 88(x2)
    lbu x5, 88(x2)
    beq x5, x0, 8
    jal x0, f3_9
    jal x0, f3_10
f3_9:
    addi x10, x0, -4
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f3_10:
    jal x0, f3_11
f3_11:
    addi x5, x0, 0
    sd x5, 96(x2)
    jal x0, f3_13
f3_13:
    ld x5, 96(x2)
    ld x6, 64(x2)
    sltu x5, x5, x6
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f3_14
    jal x0, f3_15
f3_14:
    ld x5, 96(x2)
    sd x5, 112(x2)
    addi x5, x0, 0
    ld x28, 112(x2)
    ld x6, 8(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    sd x5, 0(x7)
    ld x5, 96(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 120(x2)
    ld x5, 120(x2)
    sd x5, 96(x2)
    jal x0, f3_13
f3_15:
    ld x5, 16(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 128(x2)
    ld x5, 128(x2)
    sd x5, 136(x2)
    ld x5, 136(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltu x5, x0, x5
    sb x5, 144(x2)
    lbu x5, 144(x2)
    beq x5, x0, 8
    jal x0, f3_16
    jal x0, f3_17
f3_16:
    addi x5, x0, 0
    xori x5, x5, -1
    sd x5, 152(x2)
    ld x5, 152(x2)
    ld x6, 136(x2)
    andi x6, x6, 63
    sll x5, x5, x6
    sd x5, 160(x2)
    ld x5, 64(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 168(x2)
    ld x5, 168(x2)
    sd x5, 176(x2)
    ld x5, 160(x2)
    ld x28, 176(x2)
    ld x6, 8(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    sd x5, 0(x7)
    jal x0, f3_18
f3_17:
    jal x0, f3_18
f3_18:
    ld x5, 16(x2)
    ld x7, 0(x2)
    sd x5, 0(x7)
    ld x5, 16(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    addi x5, x0, 0
    ld x7, 0(x2)
    sd x5, 16(x7)
    addi x10, x0, 0
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
# gen_op
fn_4:
    addi x2, x2, -448
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 440(x2)
    sd x10, 432(x2)
    sd x11, 0(x2)
    sd x12, 8(x2)
    sd x13, 16(x2)
f4_0:
    ld x10, 0(x2)
    jal x1, fn_12
    sd x10, 48(x2)
    jal x0, f4_1
f4_1:
    ld x5, 48(x2)
    addi x6, x0, 8
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 56(x2)
    ld x5, 56(x2)
    sd x5, 64(x2)
    ld x10, 0(x2)
    jal x1, fn_12
    sd x10, 72(x2)
    jal x0, f4_2
f4_2:
    ld x5, 72(x2)
    sd x5, 80(x2)
    ld x10, 0(x2)
    jal x1, fn_12
    sd x10, 88(x2)
    jal x0, f4_3
f4_3:
    ld x5, 88(x2)
    sd x5, 96(x2)
    ld x5, 16(x2)
    addi x6, x0, 128
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 104(x2)
    ld x5, 104(x2)
    addi x6, x0, 2
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 112(x2)
    ld x5, 112(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 120(x2)
    lbu x5, 120(x2)
    sb x5, 128(x2)
    ld x5, 64(x2)
    addi x6, x0, 3
    sltu x5, x5, x6
    sb x5, 136(x2)
    lbu x5, 136(x2)
    beq x5, x0, 8
    jal x0, f4_4
    jal x0, f4_5
f4_4:
    lbu x5, 128(x2)
    beq x5, x0, 8
    jal x0, f4_7
    jal x0, f4_8
f4_5:
    jal x0, f4_6
f4_6:
    ld x5, 64(x2)
    addi x6, x0, 5
    sltu x5, x5, x6
    sb x5, 208(x2)
    lbu x5, 208(x2)
    beq x5, x0, 8
    jal x0, f4_12
    jal x0, f4_13
f4_7:
    ld x5, 8(x2)
    addi x6, x0, 2
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 144(x2)
    ld x5, 80(x2)
    ld x6, 144(x2)
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 152(x2)
    addi x5, x0, 1
    sd x5, 160(x2)
    ld x5, 152(x2)
    sd x5, 168(x2)
    addi x5, x0, 0
    sd x5, 176(x2)
    ld x6, 432(x2)
    addi x7, x2, 160
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 440(x2)
    addi x2, x2, 448
    jalr x0, 0(x1)
f4_8:
    jal x0, f4_9
f4_9:
    addi x5, x0, 0
    sd x5, 184(x2)
    addi x5, x0, 0
    sd x5, 192(x2)
    addi x5, x0, 0
    sd x5, 200(x2)
    ld x6, 432(x2)
    addi x7, x2, 184
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 440(x2)
    addi x2, x2, 448
    jalr x0, 0(x1)
f4_12:
    lbu x5, 128(x2)
    beq x5, x0, 8
    jal x0, f4_15
    jal x0, f4_16
f4_13:
    jal x0, f4_14
f4_14:
    ld x5, 64(x2)
    addi x6, x0, 5
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 280(x2)
    lbu x5, 280(x2)
    beq x5, x0, 8
    jal x0, f4_20
    jal x0, f4_21
f4_15:
    addi x5, x0, 0
    sd x5, 216(x2)
    addi x5, x0, 0
    sd x5, 224(x2)
    addi x5, x0, 0
    sd x5, 232(x2)
    ld x6, 432(x2)
    addi x7, x2, 216
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 440(x2)
    addi x2, x2, 448
    jalr x0, 0(x1)
f4_16:
    jal x0, f4_17
f4_17:
    ld x5, 8(x2)
    addi x6, x0, 2
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 240(x2)
    ld x5, 80(x2)
    ld x6, 240(x2)
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 248(x2)
    addi x5, x0, 1
    sd x5, 256(x2)
    ld x5, 248(x2)
    sd x5, 264(x2)
    addi x5, x0, 0
    sd x5, 272(x2)
    ld x6, 432(x2)
    addi x7, x2, 256
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 440(x2)
    addi x2, x2, 448
    jalr x0, 0(x1)
f4_20:
    ld x5, 80(x2)
    addi x6, x0, 6
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 288(x2)
    addi x5, x0, 2
    sd x5, 296(x2)
    ld x5, 288(x2)
    sd x5, 304(x2)
    addi x5, x0, 0
    sd x5, 312(x2)
    ld x6, 432(x2)
    addi x7, x2, 296
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 440(x2)
    addi x2, x2, 448
    jalr x0, 0(x1)
f4_21:
    jal x0, f4_22
f4_22:
    ld x5, 64(x2)
    addi x6, x0, 6
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 320(x2)
    lbu x5, 320(x2)
    beq x5, x0, 8
    jal x0, f4_24
    jal x0, f4_25
f4_24:
    ld x5, 8(x2)
    addi x6, x0, 2
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 328(x2)
    ld x5, 80(x2)
    ld x6, 328(x2)
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 336(x2)
    ld x5, 96(x2)
    addi x6, x0, 5
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 344(x2)
    addi x5, x0, 3
    sd x5, 352(x2)
    ld x5, 336(x2)
    sd x5, 360(x2)
    ld x5, 344(x2)
    sd x5, 368(x2)
    ld x6, 432(x2)
    addi x7, x2, 352
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 440(x2)
    addi x2, x2, 448
    jalr x0, 0(x1)
f4_25:
    jal x0, f4_26
f4_26:
    ld x5, 8(x2)
    addi x6, x0, 2
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 376(x2)
    ld x5, 80(x2)
    ld x6, 376(x2)
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 384(x2)
    ld x5, 96(x2)
    addi x6, x0, 5
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 392(x2)
    addi x5, x0, 4
    sd x5, 400(x2)
    ld x5, 384(x2)
    sd x5, 408(x2)
    ld x5, 392(x2)
    sd x5, 416(x2)
    ld x6, 432(x2)
    addi x7, x2, 400
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x28, 16(x7)
    sd x28, 16(x6)
    ld x1, 440(x2)
    addi x2, x2, 448
    jalr x0, 0(x1)
# apply_ref
fn_5:
    addi x2, x2, -192
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 184(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f5_0:
    ld x7, 8(x2)
    ld x5, 0(x7)
    sd x5, 24(x2)
    ld x5, 24(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 32(x2)
    lbu x5, 32(x2)
    beq x5, x0, 8
    jal x0, f5_1
    jal x0, f5_2
f5_1:
    ld x10, 0(x2)
    jal x1, fn_13
    sd x10, 40(x2)
    jal x0, f5_4
f5_2:
    jal x0, f5_3
f5_3:
    ld x7, 8(x2)
    ld x5, 0(x7)
    sd x5, 48(x2)
    ld x5, 48(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 56(x2)
    lbu x5, 56(x2)
    beq x5, x0, 8
    jal x0, f5_6
    jal x0, f5_7
f5_4:
    ld x10, 40(x2)
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
f5_6:
    ld x7, 8(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 64(x2)
    ld x10, 0(x2)
    ld x11, 64(x2)
    jal x1, fn_14
    sd x10, 72(x2)
    jal x0, f5_9
f5_7:
    jal x0, f5_8
f5_8:
    ld x7, 8(x2)
    ld x5, 0(x7)
    sd x5, 80(x2)
    ld x5, 80(x2)
    addi x6, x0, 2
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 88(x2)
    lbu x5, 88(x2)
    beq x5, x0, 8
    jal x0, f5_11
    jal x0, f5_12
f5_9:
    ld x10, 72(x2)
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
f5_11:
    ld x7, 8(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 96(x2)
    ld x10, 0(x2)
    ld x11, 96(x2)
    jal x1, fn_15
    sd x10, 104(x2)
    jal x0, f5_14
f5_12:
    jal x0, f5_13
f5_13:
    ld x7, 8(x2)
    ld x5, 0(x7)
    sd x5, 112(x2)
    ld x5, 112(x2)
    addi x6, x0, 3
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 120(x2)
    lbu x5, 120(x2)
    beq x5, x0, 8
    jal x0, f5_16
    jal x0, f5_17
f5_14:
    ld x10, 104(x2)
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
f5_16:
    ld x7, 8(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 128(x2)
    ld x7, 8(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 136(x2)
    ld x10, 0(x2)
    ld x11, 128(x2)
    ld x12, 136(x2)
    jal x1, fn_16
    sd x10, 144(x2)
    jal x0, f5_19
f5_17:
    jal x0, f5_18
f5_18:
    ld x7, 8(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 152(x2)
    ld x7, 8(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 160(x2)
    ld x10, 0(x2)
    ld x11, 152(x2)
    ld x12, 160(x2)
    jal x1, fn_17
    sd x10, 168(x2)
    jal x0, f5_21
f5_19:
    ld x10, 144(x2)
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
f5_21:
    ld x10, 168(x2)
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
# apply_sut
fn_6:
    addi x2, x2, -208
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 200(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sd x13, 24(x2)
f6_0:
    ld x7, 24(x2)
    ld x5, 0(x7)
    sd x5, 40(x2)
    ld x5, 40(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f6_1
    jal x0, f6_2
f6_1:
    ld x10, 8(x2)
    ld x11, 16(x2)
    jal x1, fn_18
    sd x10, 56(x2)
    jal x0, f6_4
f6_2:
    jal x0, f6_3
f6_3:
    ld x7, 24(x2)
    ld x5, 0(x7)
    sd x5, 64(x2)
    ld x5, 64(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 72(x2)
    lbu x5, 72(x2)
    beq x5, x0, 8
    jal x0, f6_6
    jal x0, f6_7
f6_4:
    ld x10, 56(x2)
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f6_6:
    ld x7, 24(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 80(x2)
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    ld x13, 80(x2)
    jal x1, fn_19
    sd x10, 88(x2)
    jal x0, f6_9
f6_7:
    jal x0, f6_8
f6_8:
    ld x7, 24(x2)
    ld x5, 0(x7)
    sd x5, 96(x2)
    ld x5, 96(x2)
    addi x6, x0, 2
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f6_11
    jal x0, f6_12
f6_9:
    ld x10, 88(x2)
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f6_11:
    ld x7, 24(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 112(x2)
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    ld x13, 112(x2)
    jal x1, fn_20
    sd x10, 120(x2)
    jal x0, f6_14
f6_12:
    jal x0, f6_13
f6_13:
    ld x7, 24(x2)
    ld x5, 0(x7)
    sd x5, 128(x2)
    ld x5, 128(x2)
    addi x6, x0, 3
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 136(x2)
    lbu x5, 136(x2)
    beq x5, x0, 8
    jal x0, f6_16
    jal x0, f6_17
f6_14:
    ld x10, 120(x2)
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f6_16:
    ld x7, 24(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 144(x2)
    ld x7, 24(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 152(x2)
    ld x10, 8(x2)
    ld x11, 16(x2)
    ld x12, 144(x2)
    ld x13, 152(x2)
    jal x1, fn_21
    sd x10, 160(x2)
    jal x0, f6_19
f6_17:
    jal x0, f6_18
f6_18:
    ld x7, 24(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 168(x2)
    ld x7, 24(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 176(x2)
    ld x10, 8(x2)
    ld x11, 16(x2)
    ld x12, 168(x2)
    ld x13, 176(x2)
    jal x1, fn_22
    sd x10, 184(x2)
    jal x0, f6_21
f6_19:
    ld x10, 160(x2)
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f6_21:
    ld x10, 184(x2)
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
# show_op
fn_7:
    addi x2, x2, -208
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 200(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f7_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 16(x2)
    ld x5, 16(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 24(x2)
    lbu x5, 24(x2)
    beq x5, x0, 8
    jal x0, f7_1
    jal x0, f7_2
f7_1:
    jal x5, 20
    .word 1869376609
    .word 1043144803
    .word 1819026720
    .word 2660
    sd x5, 32(x2)
    jal x10, 16
    .word 1869376609
    .word 1043144803
    .word 32
    jal x1, rt_puts
    ld x10, 8(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f7_4
f7_2:
    jal x0, f7_3
f7_3:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 40(x2)
    ld x5, 40(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f7_5
    jal x0, f7_6
f7_4:
    jal x0, f7_3
f7_5:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 56(x2)
    jal x5, 24
    .word 1701147238
    .word 1819026720
    .word 1043144821
    .word 1819026720
    .word 2660
    sd x5, 64(x2)
    jal x10, 12
    .word 1701147238
    .word 32
    jal x1, rt_puts
    ld x10, 56(x2)
    jal x1, rt_put_u64
    jal x10, 12
    .word 540945696
    .word 0
    jal x1, rt_puts
    ld x10, 8(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f7_8
f7_6:
    jal x0, f7_7
f7_7:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 72(x2)
    ld x5, 72(x2)
    addi x6, x0, 2
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 80(x2)
    lbu x5, 80(x2)
    beq x5, x0, 8
    jal x0, f7_9
    jal x0, f7_10
f7_8:
    jal x0, f7_7
f7_9:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 88(x2)
    jal x5, 28
    .word 1869376609
    .word 1970429795
    .word 1814372462
    .word 757101932
    .word 1814372414
    .word 681068
    sd x5, 96(x2)
    jal x10, 16
    .word 1869376609
    .word 1970429795
    .word 8302
    jal x1, rt_puts
    ld x10, 88(x2)
    jal x1, rt_put_u64
    jal x10, 12
    .word 540945696
    .word 0
    jal x1, rt_puts
    ld x10, 8(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f7_12
f7_10:
    jal x0, f7_11
f7_11:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 104(x2)
    ld x5, 104(x2)
    addi x6, x0, 3
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 112(x2)
    lbu x5, 112(x2)
    beq x5, x0, 8
    jal x0, f7_13
    jal x0, f7_14
f7_12:
    jal x0, f7_11
f7_13:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 120(x2)
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 128(x2)
    jal x5, 32
    .word 1702061426
    .word 543520370
    .word 1970039845
    .word 1819026720
    .word 1043144821
    .word 1819026720
    .word 2660
    sd x5, 136(x2)
    jal x10, 16
    .word 1702061426
    .word 543520370
    .word 0
    jal x1, rt_puts
    ld x10, 120(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    ld x10, 128(x2)
    jal x1, rt_put_u64
    jal x10, 12
    .word 540945696
    .word 0
    jal x1, rt_puts
    ld x10, 8(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f7_16
f7_14:
    jal x0, f7_15
f7_15:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 144(x2)
    ld x5, 144(x2)
    addi x6, x0, 4
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 152(x2)
    lbu x5, 152(x2)
    beq x5, x0, 8
    jal x0, f7_17
    jal x0, f7_18
f7_16:
    jal x0, f7_15
f7_17:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 160(x2)
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 168(x2)
    jal x5, 32
    .word 1701147238
    .word 1853190751
    .word 1819026720
    .word 1814372469
    .word 757101932
    .word 1814372414
    .word 681068
    sd x5, 176(x2)
    jal x10, 16
    .word 1701147238
    .word 1853190751
    .word 32
    jal x1, rt_puts
    ld x10, 160(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    ld x10, 168(x2)
    jal x1, rt_put_u64
    jal x10, 12
    .word 540945696
    .word 0
    jal x1, rt_puts
    ld x10, 8(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f7_20
f7_18:
    jal x0, f7_19
f7_19:
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f7_20:
    jal x0, f7_19
# digest_step
fn_8:
    addi x2, x2, -80
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 72(x2)
    sd x10, 0(x2)
    sb x11, 8(x2)
f8_0:
    ld x5, 0(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sd x5, 24(x2)
    ld x5, 24(x2)
    sd x5, 32(x2)
    lbu x5, 8(x2)
    beq x5, x0, 8
    jal x0, f8_1
    jal x0, f8_2
f8_1:
    ld x5, 0(x2)
    addi x6, x0, 2
    xor x5, x5, x6
    sd x5, 40(x2)
    ld x5, 40(x2)
    sd x5, 32(x2)
    jal x0, f8_3
f8_2:
    jal x0, f8_3
f8_3:
    ld x5, 32(x2)
    jal x6, 12
    .word 435
    .word 256
    ld x6, 0(x6)
    mul x5, x5, x6
    sd x5, 48(x2)
    ld x10, 48(x2)
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
# pfa_is_used
fn_9:
    addi x2, x2, -128
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 120(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f9_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 32(x2)
    ld x5, 16(x2)
    ld x6, 32(x2)
    sltu x5, x5, x6
    xori x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f9_1
    jal x0, f9_2
f9_1:
    addi x10, x0, 1
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f9_2:
    jal x0, f9_3
f9_3:
    ld x5, 16(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 48(x2)
    ld x5, 48(x2)
    sd x5, 56(x2)
    ld x28, 56(x2)
    ld x6, 8(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    ld x5, 0(x7)
    sd x5, 64(x2)
    ld x5, 16(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 72(x2)
    ld x5, 64(x2)
    ld x6, 72(x2)
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 80(x2)
    ld x5, 80(x2)
    addi x6, x0, 1
    and x5, x5, x6
    sd x5, 88(x2)
    ld x5, 88(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 96(x2)
    lbu x10, 96(x2)
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
# ref_free_count
fn_10:
    addi x2, x2, -112
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 104(x2)
    sd x10, 0(x2)
f10_0:
    addi x5, x0, 0
    sd x5, 16(x2)
    addi x5, x0, 0
    sd x5, 24(x2)
    jal x0, f10_1
f10_1:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 32(x2)
    ld x10, 32(x2)
    ld x10, 8(x10)
    sd x10, 40(x2)
    jal x0, f10_4
f10_2:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 56(x2)
    ld x28, 24(x2)
    ld x6, 56(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    add x7, x6, x28
    lbu x5, 0(x7)
    sb x5, 64(x2)
    lbu x5, 64(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 72(x2)
    lbu x5, 72(x2)
    beq x5, x0, 8
    jal x0, f10_5
    jal x0, f10_6
f10_3:
    ld x10, 16(x2)
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
f10_4:
    ld x5, 24(x2)
    ld x6, 40(x2)
    sltu x5, x5, x6
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f10_2
    jal x0, f10_3
f10_5:
    ld x5, 16(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 80(x2)
    ld x5, 80(x2)
    sd x5, 16(x2)
    jal x0, f10_7
f10_6:
    jal x0, f10_7
f10_7:
    ld x5, 24(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 88(x2)
    ld x5, 88(x2)
    sd x5, 24(x2)
    jal x0, f10_1
# pfa_words
fn_11:
    addi x2, x2, -48
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 40(x2)
    sd x10, 0(x2)
f11_0:
    ld x5, 0(x2)
    addi x6, x0, 63
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 16(x2)
    ld x5, 16(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 24(x2)
    ld x10, 24(x2)
    ld x1, 40(x2)
    addi x2, x2, 48
    jalr x0, 0(x1)
# rng_next
fn_12:
    addi x2, x2, -128
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 120(x2)
    sd x10, 0(x2)
f12_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 16(x2)
    ld x5, 16(x2)
    jal x6, 12
    .word 2135587861
    .word 2654435769
    ld x6, 0(x6)
    add x5, x5, x6
    sd x5, 24(x2)
    ld x5, 24(x2)
    ld x7, 0(x2)
    sd x5, 0(x7)
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 32(x2)
    ld x5, 32(x2)
    sd x5, 40(x2)
    ld x5, 40(x2)
    addi x6, x0, 30
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 48(x2)
    ld x5, 40(x2)
    ld x6, 48(x2)
    xor x5, x5, x6
    sd x5, 56(x2)
    ld x5, 56(x2)
    jal x6, 12
    .word 484763065
    .word 3210233709
    ld x6, 0(x6)
    mul x5, x5, x6
    sd x5, 64(x2)
    ld x5, 64(x2)
    sd x5, 40(x2)
    ld x5, 40(x2)
    addi x6, x0, 27
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 72(x2)
    ld x5, 40(x2)
    ld x6, 72(x2)
    xor x5, x5, x6
    sd x5, 80(x2)
    ld x5, 80(x2)
    jal x6, 12
    .word 321982955
    .word 2496678331
    ld x6, 0(x6)
    mul x5, x5, x6
    sd x5, 88(x2)
    ld x5, 88(x2)
    sd x5, 40(x2)
    ld x5, 40(x2)
    addi x6, x0, 31
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 96(x2)
    ld x5, 40(x2)
    ld x6, 96(x2)
    xor x5, x5, x6
    sd x5, 104(x2)
    ld x10, 104(x2)
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
# ref_alloc
fn_13:
    addi x2, x2, -112
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 104(x2)
    sd x10, 0(x2)
f13_0:
    addi x5, x0, 0
    sd x5, 16(x2)
    jal x0, f13_1
f13_1:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 24(x2)
    ld x10, 24(x2)
    ld x10, 8(x10)
    sd x10, 32(x2)
    jal x0, f13_4
f13_2:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 48(x2)
    ld x28, 16(x2)
    ld x6, 48(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    add x7, x6, x28
    lbu x5, 0(x7)
    sb x5, 56(x2)
    lbu x5, 56(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 64(x2)
    lbu x5, 64(x2)
    beq x5, x0, 8
    jal x0, f13_5
    jal x0, f13_6
f13_3:
    addi x10, x0, -1
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
f13_4:
    ld x5, 16(x2)
    ld x6, 32(x2)
    sltu x5, x5, x6
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f13_2
    jal x0, f13_3
f13_5:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 72(x2)
    addi x5, x0, 1
    ld x28, 16(x2)
    ld x6, 72(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x28, 0
    addi x11, x7, 0
    jal x0, rt_vec_oob
    add x7, x6, x28
    sb x5, 0(x7)
    jal x0, f13_8
f13_6:
    jal x0, f13_7
f13_7:
    ld x5, 16(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 88(x2)
    ld x5, 88(x2)
    sd x5, 16(x2)
    jal x0, f13_1
f13_8:
    ld x5, 16(x2)
    sd x5, 80(x2)
    ld x10, 80(x2)
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
# ref_free
fn_14:
    addi x2, x2, -112
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 104(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f14_0:
    ld x10, 0(x2)
    jal x1, fn_23
    sd x10, 24(x2)
    jal x0, f14_1
f14_1:
    ld x5, 8(x2)
    ld x6, 24(x2)
    sltu x5, x5, x6
    xori x5, x5, 1
    sb x5, 32(x2)
    lbu x5, 32(x2)
    beq x5, x0, 8
    jal x0, f14_2
    jal x0, f14_3
f14_2:
    addi x10, x0, -2
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
f14_3:
    jal x0, f14_4
f14_4:
    ld x5, 8(x2)
    sd x5, 40(x2)
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 48(x2)
    ld x28, 40(x2)
    ld x6, 48(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    add x7, x6, x28
    lbu x5, 0(x7)
    sb x5, 56(x2)
    lbu x5, 56(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 64(x2)
    lbu x5, 64(x2)
    beq x5, x0, 8
    jal x0, f14_6
    jal x0, f14_7
f14_6:
    addi x10, x0, -3
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
f14_7:
    jal x0, f14_8
f14_8:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 72(x2)
    ld x5, 8(x2)
    sd x5, 80(x2)
    addi x5, x0, 0
    ld x28, 80(x2)
    ld x6, 72(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x28, 0
    addi x11, x7, 0
    jal x0, rt_vec_oob
    add x7, x6, x28
    sb x5, 0(x7)
    jal x0, f14_10
f14_10:
    addi x10, x0, 0
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
# ref_alloc_run
fn_15:
    addi x2, x2, -128
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 120(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f15_0:
    ld x5, 8(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 24(x2)
    lbu x5, 24(x2)
    beq x5, x0, 8
    jal x0, f15_1
    jal x0, f15_2
f15_1:
    addi x10, x0, -2
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f15_2:
    jal x0, f15_3
f15_3:
    ld x10, 0(x2)
    jal x1, fn_23
    sd x10, 32(x2)
    jal x0, f15_5
f15_5:
    ld x5, 32(x2)
    sd x5, 40(x2)
    ld x5, 8(x2)
    ld x6, 40(x2)
    sltu x5, x6, x5
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f15_6
    jal x0, f15_7
f15_6:
    addi x10, x0, -1
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f15_7:
    jal x0, f15_8
f15_8:
    addi x5, x0, 0
    sd x5, 56(x2)
    jal x0, f15_10
f15_10:
    ld x5, 40(x2)
    ld x6, 8(x2)
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 64(x2)
    ld x5, 56(x2)
    ld x6, 64(x2)
    sltu x5, x6, x5
    xori x5, x5, 1
    sb x5, 72(x2)
    lbu x5, 72(x2)
    beq x5, x0, 8
    jal x0, f15_11
    jal x0, f15_12
f15_11:
    ld x10, 0(x2)
    ld x11, 56(x2)
    ld x12, 8(x2)
    addi x13, x0, 0
    jal x1, fn_24
    sb x10, 80(x2)
    jal x0, f15_13
f15_12:
    addi x10, x0, -1
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f15_13:
    lbu x5, 80(x2)
    beq x5, x0, 8
    jal x0, f15_14
    jal x0, f15_15
f15_14:
    ld x10, 0(x2)
    ld x11, 56(x2)
    ld x12, 8(x2)
    addi x13, x0, 1
    jal x1, fn_25
    jal x0, f15_17
f15_15:
    jal x0, f15_16
f15_16:
    ld x5, 56(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 96(x2)
    ld x5, 96(x2)
    sd x5, 56(x2)
    jal x0, f15_10
f15_17:
    ld x5, 56(x2)
    sd x5, 88(x2)
    ld x10, 88(x2)
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
# ref_reserve
fn_16:
    addi x2, x2, -80
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 72(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f16_0:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    jal x1, fn_26
    sb x10, 32(x2)
    jal x0, f16_1
f16_1:
    lbu x5, 32(x2)
    xori x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f16_2
    jal x0, f16_3
f16_2:
    addi x10, x0, -2
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f16_3:
    jal x0, f16_4
f16_4:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    addi x13, x0, 0
    jal x1, fn_24
    sb x10, 48(x2)
    jal x0, f16_6
f16_6:
    lbu x5, 48(x2)
    xori x5, x5, 1
    sb x5, 56(x2)
    lbu x5, 56(x2)
    beq x5, x0, 8
    jal x0, f16_7
    jal x0, f16_8
f16_7:
    addi x10, x0, -3
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f16_8:
    jal x0, f16_9
f16_9:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    addi x13, x0, 1
    jal x1, fn_25
    jal x0, f16_11
f16_11:
    addi x10, x0, 0
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
# ref_free_run
fn_17:
    addi x2, x2, -80
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 72(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f17_0:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    jal x1, fn_26
    sb x10, 32(x2)
    jal x0, f17_1
f17_1:
    lbu x5, 32(x2)
    xori x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f17_2
    jal x0, f17_3
f17_2:
    addi x10, x0, -2
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f17_3:
    jal x0, f17_4
f17_4:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    addi x13, x0, 1
    jal x1, fn_24
    sb x10, 48(x2)
    jal x0, f17_6
f17_6:
    lbu x5, 48(x2)
    xori x5, x5, 1
    sb x5, 56(x2)
    lbu x5, 56(x2)
    beq x5, x0, 8
    jal x0, f17_7
    jal x0, f17_8
f17_7:
    addi x10, x0, -3
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f17_8:
    jal x0, f17_9
f17_9:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    addi x13, x0, 0
    jal x1, fn_25
    jal x0, f17_11
f17_11:
    addi x10, x0, 0
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
# pfa_alloc
fn_18:
    addi x2, x2, -192
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 184(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f18_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 24(x2)
    ld x10, 24(x2)
    jal x1, fn_11
    sd x10, 32(x2)
    jal x0, f18_1
f18_1:
    ld x5, 32(x2)
    sd x5, 40(x2)
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 48(x2)
    ld x5, 48(x2)
    sd x5, 56(x2)
    jal x0, f18_2
f18_2:
    ld x5, 56(x2)
    ld x6, 40(x2)
    sltu x5, x5, x6
    sb x5, 64(x2)
    lbu x5, 64(x2)
    beq x5, x0, 8
    jal x0, f18_3
    jal x0, f18_4
f18_3:
    ld x5, 56(x2)
    sd x5, 72(x2)
    ld x28, 72(x2)
    ld x6, 8(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    ld x5, 0(x7)
    sd x5, 80(x2)
    ld x5, 80(x2)
    xori x5, x5, -1
    sd x5, 88(x2)
    ld x5, 88(x2)
    sd x5, 96(x2)
    ld x5, 96(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltu x5, x0, x5
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f18_5
    jal x0, f18_6
f18_4:
    ld x5, 40(x2)
    ld x7, 0(x2)
    sd x5, 16(x7)
    addi x10, x0, -1
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
f18_5:
    ld x5, 56(x2)
    addi x6, x0, 64
    mulhu x7, x5, x6
    mul x5, x5, x6
    beq x7, x0, 8
    jal x0, rt_ovf_mul
    sd x5, 112(x2)
    ld x10, 96(x2)
    jal x1, fn_27
    sd x10, 120(x2)
    jal x0, f18_8
f18_6:
    jal x0, f18_7
f18_7:
    ld x5, 56(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 168(x2)
    ld x5, 168(x2)
    sd x5, 56(x2)
    jal x0, f18_2
f18_8:
    ld x5, 112(x2)
    ld x6, 120(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 128(x2)
    ld x5, 128(x2)
    sd x5, 136(x2)
    ld x10, 8(x2)
    ld x11, 136(x2)
    jal x1, fn_28
    jal x0, f18_9
f18_9:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 144(x2)
    ld x5, 144(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 152(x2)
    ld x5, 152(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    ld x5, 56(x2)
    ld x7, 0(x2)
    sd x5, 16(x7)
    ld x5, 136(x2)
    sd x5, 160(x2)
    ld x10, 160(x2)
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
# sut_free
fn_19:
    addi x2, x2, -96
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 88(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sd x13, 24(x2)
f19_0:
    ld x5, 0(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f19_1
    jal x0, f19_2
f19_1:
    ld x10, 8(x2)
    ld x11, 16(x2)
    ld x12, 24(x2)
    jal x1, fn_29
    sd x10, 48(x2)
    jal x0, f19_4
f19_2:
    jal x0, f19_3
f19_3:
    ld x5, 0(x2)
    addi x6, x0, 2
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 56(x2)
    lbu x5, 56(x2)
    beq x5, x0, 8
    jal x0, f19_6
    jal x0, f19_7
f19_4:
    ld x10, 48(x2)
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
f19_6:
    ld x10, 8(x2)
    ld x11, 16(x2)
    ld x12, 24(x2)
    jal x1, fn_30
    sd x10, 64(x2)
    jal x0, f19_9
f19_7:
    jal x0, f19_8
f19_8:
    ld x10, 8(x2)
    ld x11, 16(x2)
    ld x12, 24(x2)
    jal x1, fn_31
    sd x10, 72(x2)
    jal x0, f19_11
f19_9:
    ld x10, 64(x2)
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
f19_11:
    ld x10, 72(x2)
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
# sut_alloc_run
fn_20:
    addi x2, x2, -80
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 72(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sd x13, 24(x2)
f20_0:
    ld x5, 0(x2)
    addi x6, x0, 3
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f20_1
    jal x0, f20_2
f20_1:
    ld x10, 8(x2)
    ld x11, 16(x2)
    ld x12, 24(x2)
    jal x1, fn_32
    sd x10, 48(x2)
    jal x0, f20_4
f20_2:
    jal x0, f20_3
f20_3:
    ld x10, 8(x2)
    ld x11, 16(x2)
    ld x12, 24(x2)
    jal x1, fn_33
    sd x10, 56(x2)
    jal x0, f20_6
f20_4:
    ld x10, 48(x2)
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f20_6:
    ld x10, 56(x2)
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
# pfa_reserve
fn_21:
    addi x2, x2, -160
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 152(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sd x13, 24(x2)
f21_0:
    ld x10, 0(x2)
    ld x11, 16(x2)
    ld x12, 24(x2)
    jal x1, fn_34
    sb x10, 40(x2)
    jal x0, f21_1
f21_1:
    lbu x5, 40(x2)
    xori x5, x5, 1
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f21_2
    jal x0, f21_3
f21_2:
    addi x10, x0, -2
    ld x1, 152(x2)
    addi x2, x2, 160
    jalr x0, 0(x1)
f21_3:
    jal x0, f21_4
f21_4:
    ld x5, 16(x2)
    sd x5, 56(x2)
    jal x0, f21_6
f21_6:
    ld x5, 16(x2)
    ld x6, 24(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 64(x2)
    ld x5, 56(x2)
    ld x6, 64(x2)
    sltu x5, x5, x6
    sb x5, 72(x2)
    lbu x5, 72(x2)
    beq x5, x0, 8
    jal x0, f21_7
    jal x0, f21_8
f21_7:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 56(x2)
    jal x1, fn_9
    sb x10, 80(x2)
    jal x0, f21_9
f21_8:
    ld x5, 16(x2)
    sd x5, 56(x2)
    jal x0, f21_14
f21_9:
    lbu x5, 80(x2)
    beq x5, x0, 8
    jal x0, f21_10
    jal x0, f21_11
f21_10:
    addi x10, x0, -3
    ld x1, 152(x2)
    addi x2, x2, 160
    jalr x0, 0(x1)
f21_11:
    jal x0, f21_12
f21_12:
    ld x5, 56(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 88(x2)
    ld x5, 88(x2)
    sd x5, 56(x2)
    jal x0, f21_6
f21_14:
    ld x5, 16(x2)
    ld x6, 24(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 96(x2)
    ld x5, 56(x2)
    ld x6, 96(x2)
    sltu x5, x5, x6
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f21_15
    jal x0, f21_16
f21_15:
    ld x10, 8(x2)
    ld x11, 56(x2)
    jal x1, fn_28
    jal x0, f21_17
f21_16:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 120(x2)
    ld x5, 120(x2)
    ld x6, 24(x2)
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 128(x2)
    ld x5, 128(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    addi x10, x0, 0
    ld x1, 152(x2)
    addi x2, x2, 160
    jalr x0, 0(x1)
f21_17:
    ld x5, 56(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 112(x2)
    ld x5, 112(x2)
    sd x5, 56(x2)
    jal x0, f21_14
# pfa_free_run
fn_22:
    addi x2, x2, -192
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 184(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sd x13, 24(x2)
f22_0:
    ld x10, 0(x2)
    ld x11, 16(x2)
    ld x12, 24(x2)
    jal x1, fn_34
    sb x10, 40(x2)
    jal x0, f22_1
f22_1:
    lbu x5, 40(x2)
    xori x5, x5, 1
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f22_2
    jal x0, f22_3
f22_2:
    addi x10, x0, -2
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
f22_3:
    jal x0, f22_4
f22_4:
    ld x5, 16(x2)
    sd x5, 56(x2)
    jal x0, f22_6
f22_6:
    ld x5, 16(x2)
    ld x6, 24(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 64(x2)
    ld x5, 56(x2)
    ld x6, 64(x2)
    sltu x5, x5, x6
    sb x5, 72(x2)
    lbu x5, 72(x2)
    beq x5, x0, 8
    jal x0, f22_7
    jal x0, f22_8
f22_7:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 56(x2)
    jal x1, fn_9
    sb x10, 80(x2)
    jal x0, f22_9
f22_8:
    ld x5, 16(x2)
    sd x5, 56(x2)
    jal x0, f22_14
f22_9:
    lbu x5, 80(x2)
    xori x5, x5, 1
    sb x5, 88(x2)
    lbu x5, 88(x2)
    beq x5, x0, 8
    jal x0, f22_10
    jal x0, f22_11
f22_10:
    addi x10, x0, -3
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
f22_11:
    jal x0, f22_12
f22_12:
    ld x5, 56(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 96(x2)
    ld x5, 96(x2)
    sd x5, 56(x2)
    jal x0, f22_6
f22_14:
    ld x5, 16(x2)
    ld x6, 24(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 104(x2)
    ld x5, 56(x2)
    ld x6, 104(x2)
    sltu x5, x5, x6
    sb x5, 112(x2)
    lbu x5, 112(x2)
    beq x5, x0, 8
    jal x0, f22_15
    jal x0, f22_16
f22_15:
    ld x10, 8(x2)
    ld x11, 56(x2)
    jal x1, fn_35
    jal x0, f22_17
f22_16:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 128(x2)
    ld x5, 128(x2)
    ld x6, 24(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 136(x2)
    ld x5, 136(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    ld x5, 16(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 144(x2)
    ld x5, 144(x2)
    sd x5, 152(x2)
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 160(x2)
    ld x5, 152(x2)
    ld x6, 160(x2)
    sltu x5, x5, x6
    sb x5, 168(x2)
    lbu x5, 168(x2)
    beq x5, x0, 8
    jal x0, f22_18
    jal x0, f22_19
f22_17:
    ld x5, 56(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 120(x2)
    ld x5, 120(x2)
    sd x5, 56(x2)
    jal x0, f22_14
f22_18:
    ld x5, 152(x2)
    ld x7, 0(x2)
    sd x5, 16(x7)
    jal x0, f22_20
f22_19:
    jal x0, f22_20
f22_20:
    addi x10, x0, 0
    ld x1, 184(x2)
    addi x2, x2, 192
    jalr x0, 0(x1)
# ref_count
fn_23:
    addi x2, x2, -64
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 56(x2)
    sd x10, 0(x2)
f23_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 16(x2)
    ld x10, 16(x2)
    ld x10, 8(x10)
    sd x10, 24(x2)
    jal x0, f23_1
f23_1:
    ld x5, 24(x2)
    sd x5, 32(x2)
    ld x10, 32(x2)
    ld x1, 56(x2)
    addi x2, x2, 64
    jalr x0, 0(x1)
# ref_all
fn_24:
    addi x2, x2, -128
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 120(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sb x13, 24(x2)
f24_0:
    addi x5, x0, 0
    sd x5, 40(x2)
    jal x0, f24_1
f24_1:
    ld x5, 40(x2)
    ld x6, 16(x2)
    sltu x5, x5, x6
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f24_2
    jal x0, f24_3
f24_2:
    ld x5, 8(x2)
    ld x6, 40(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 56(x2)
    ld x5, 56(x2)
    sd x5, 64(x2)
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 72(x2)
    ld x28, 64(x2)
    ld x6, 72(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    add x7, x6, x28
    lbu x5, 0(x7)
    sb x5, 80(x2)
    lbu x5, 80(x2)
    lbu x6, 24(x2)
    xor x5, x5, x6
    sltu x5, x0, x5
    sb x5, 88(x2)
    lbu x5, 88(x2)
    beq x5, x0, 8
    jal x0, f24_4
    jal x0, f24_5
f24_3:
    addi x10, x0, 1
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f24_4:
    addi x10, x0, 0
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f24_5:
    jal x0, f24_6
f24_6:
    ld x5, 40(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 96(x2)
    ld x5, 96(x2)
    sd x5, 40(x2)
    jal x0, f24_1
# ref_fill
fn_25:
    addi x2, x2, -96
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 88(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
    sb x13, 24(x2)
f25_0:
    addi x5, x0, 0
    sd x5, 32(x2)
    jal x0, f25_1
f25_1:
    ld x5, 32(x2)
    ld x6, 16(x2)
    sltu x5, x5, x6
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f25_2
    jal x0, f25_3
f25_2:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 48(x2)
    ld x5, 8(x2)
    ld x6, 32(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 56(x2)
    ld x5, 56(x2)
    sd x5, 64(x2)
    lbu x5, 24(x2)
    ld x28, 64(x2)
    ld x6, 48(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x28, 0
    addi x11, x7, 0
    jal x0, rt_vec_oob
    add x7, x6, x28
    sb x5, 0(x7)
    jal x0, f25_4
f25_3:
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
f25_4:
    ld x5, 32(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 72(x2)
    ld x5, 72(x2)
    sd x5, 32(x2)
    jal x0, f25_1
# ref_range_ok
fn_26:
    addi x2, x2, -112
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 104(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f26_0:
    ld x10, 0(x2)
    jal x1, fn_23
    sd x10, 32(x2)
    jal x0, f26_1
f26_1:
    ld x5, 32(x2)
    sd x5, 40(x2)
    ld x5, 16(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltu x5, x0, x5
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f26_2
    jal x0, f26_3
f26_2:
    ld x5, 8(x2)
    ld x6, 40(x2)
    sltu x5, x5, x6
    sb x5, 64(x2)
    lbu x5, 64(x2)
    sb x5, 56(x2)
    jal x0, f26_4
f26_3:
    addi x5, x0, 0
    sb x5, 56(x2)
    jal x0, f26_4
f26_4:
    lbu x5, 56(x2)
    beq x5, x0, 8
    jal x0, f26_5
    jal x0, f26_6
f26_5:
    ld x5, 40(x2)
    ld x6, 8(x2)
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 80(x2)
    ld x5, 16(x2)
    ld x6, 80(x2)
    sltu x5, x6, x5
    xori x5, x5, 1
    sb x5, 88(x2)
    lbu x5, 88(x2)
    sb x5, 72(x2)
    jal x0, f26_7
f26_6:
    addi x5, x0, 0
    sb x5, 72(x2)
    jal x0, f26_7
f26_7:
    lbu x10, 72(x2)
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
# pfa_ctz
fn_27:
    addi x2, x2, -240
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 232(x2)
    sd x10, 0(x2)
f27_0:
    ld x5, 0(x2)
    sd x5, 16(x2)
    addi x5, x0, 0
    sd x5, 24(x2)
    ld x5, 16(x2)
    jal x6, 12
    .word 4294967295
    .word 0
    ld x6, 0(x6)
    and x5, x5, x6
    sd x5, 32(x2)
    ld x5, 32(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f27_1
    jal x0, f27_2
f27_1:
    ld x5, 24(x2)
    addi x6, x0, 32
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 48(x2)
    ld x5, 48(x2)
    sd x5, 24(x2)
    ld x5, 16(x2)
    addi x6, x0, 32
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 56(x2)
    ld x5, 56(x2)
    sd x5, 16(x2)
    jal x0, f27_3
f27_2:
    jal x0, f27_3
f27_3:
    ld x5, 16(x2)
    jal x6, 12
    .word 65535
    .word 0
    ld x6, 0(x6)
    and x5, x5, x6
    sd x5, 64(x2)
    ld x5, 64(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 72(x2)
    lbu x5, 72(x2)
    beq x5, x0, 8
    jal x0, f27_4
    jal x0, f27_5
f27_4:
    ld x5, 24(x2)
    addi x6, x0, 16
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 80(x2)
    ld x5, 80(x2)
    sd x5, 24(x2)
    ld x5, 16(x2)
    addi x6, x0, 16
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 88(x2)
    ld x5, 88(x2)
    sd x5, 16(x2)
    jal x0, f27_6
f27_5:
    jal x0, f27_6
f27_6:
    ld x5, 16(x2)
    addi x6, x0, 255
    and x5, x5, x6
    sd x5, 96(x2)
    ld x5, 96(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f27_7
    jal x0, f27_8
f27_7:
    ld x5, 24(x2)
    addi x6, x0, 8
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 112(x2)
    ld x5, 112(x2)
    sd x5, 24(x2)
    ld x5, 16(x2)
    addi x6, x0, 8
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 120(x2)
    ld x5, 120(x2)
    sd x5, 16(x2)
    jal x0, f27_9
f27_8:
    jal x0, f27_9
f27_9:
    ld x5, 16(x2)
    addi x6, x0, 15
    and x5, x5, x6
    sd x5, 128(x2)
    ld x5, 128(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 136(x2)
    lbu x5, 136(x2)
    beq x5, x0, 8
    jal x0, f27_10
    jal x0, f27_11
f27_10:
    ld x5, 24(x2)
    addi x6, x0, 4
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 144(x2)
    ld x5, 144(x2)
    sd x5, 24(x2)
    ld x5, 16(x2)
    addi x6, x0, 4
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 152(x2)
    ld x5, 152(x2)
    sd x5, 16(x2)
    jal x0, f27_12
f27_11:
    jal x0, f27_12
f27_12:
    ld x5, 16(x2)
    addi x6, x0, 3
    and x5, x5, x6
    sd x5, 160(x2)
    ld x5, 160(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 168(x2)
    lbu x5, 168(x2)
    beq x5, x0, 8
    jal x0, f27_13
    jal x0, f27_14
f27_13:
    ld x5, 24(x2)
    addi x6, x0, 2
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 176(x2)
    ld x5, 176(x2)
    sd x5, 24(x2)
    ld x5, 16(x2)
    addi x6, x0, 2
    andi x6, x6, 63
    srl x5, x5, x6
    sd x5, 184(x2)
    ld x5, 184(x2)
    sd x5, 16(x2)
    jal x0, f27_15
f27_14:
    jal x0, f27_15
f27_15:
    ld x5, 16(x2)
    addi x6, x0, 1
    and x5, x5, x6
    sd x5, 192(x2)
    ld x5, 192(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 200(x2)
    lbu x5, 200(x2)
    beq x5, x0, 8
    jal x0, f27_16
    jal x0, f27_17
f27_16:
    ld x5, 24(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 208(x2)
    ld x5, 208(x2)
    sd x5, 24(x2)
    jal x0, f27_18
f27_17:
    jal x0, f27_18
f27_18:
    ld x10, 24(x2)
    ld x1, 232(x2)
    addi x2, x2, 240
    jalr x0, 0(x1)
# pfa_set
fn_28:
    addi x2, x2, -96
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 88(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f28_0:
    ld x5, 8(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 16(x2)
    ld x5, 16(x2)
    sd x5, 24(x2)
    ld x5, 24(x2)
    sd x5, 32(x2)
    ld x28, 32(x2)
    ld x6, 0(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    ld x5, 0(x7)
    sd x5, 40(x2)
    ld x5, 8(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 48(x2)
    addi x5, x0, 1
    ld x6, 48(x2)
    andi x6, x6, 63
    sll x5, x5, x6
    sd x5, 56(x2)
    ld x5, 40(x2)
    ld x6, 56(x2)
    or x5, x5, x6
    sd x5, 64(x2)
    ld x5, 64(x2)
    ld x28, 32(x2)
    ld x6, 0(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    sd x5, 0(x7)
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
# ctl_free_keeps_hint
fn_29:
    addi x2, x2, -96
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 88(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f29_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 32(x2)
    ld x5, 16(x2)
    ld x6, 32(x2)
    sltu x5, x5, x6
    xori x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f29_1
    jal x0, f29_2
f29_1:
    addi x10, x0, -2
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
f29_2:
    jal x0, f29_3
f29_3:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    jal x1, fn_9
    sb x10, 48(x2)
    jal x0, f29_5
f29_5:
    lbu x5, 48(x2)
    xori x5, x5, 1
    sb x5, 56(x2)
    lbu x5, 56(x2)
    beq x5, x0, 8
    jal x0, f29_6
    jal x0, f29_7
f29_6:
    addi x10, x0, -3
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
f29_7:
    jal x0, f29_8
f29_8:
    ld x10, 8(x2)
    ld x11, 16(x2)
    jal x1, fn_35
    jal x0, f29_10
f29_10:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 64(x2)
    ld x5, 64(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 72(x2)
    ld x5, 72(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    addi x10, x0, 0
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
# ctl_free_double
fn_30:
    addi x2, x2, -112
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 104(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f30_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 32(x2)
    ld x5, 16(x2)
    ld x6, 32(x2)
    sltu x5, x5, x6
    xori x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f30_1
    jal x0, f30_2
f30_1:
    addi x10, x0, -2
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
f30_2:
    jal x0, f30_3
f30_3:
    ld x10, 8(x2)
    ld x11, 16(x2)
    jal x1, fn_35
    jal x0, f30_5
f30_5:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 48(x2)
    ld x5, 48(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 56(x2)
    ld x5, 56(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    ld x5, 16(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 64(x2)
    ld x5, 64(x2)
    sd x5, 72(x2)
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 80(x2)
    ld x5, 72(x2)
    ld x6, 80(x2)
    sltu x5, x5, x6
    sb x5, 88(x2)
    lbu x5, 88(x2)
    beq x5, x0, 8
    jal x0, f30_6
    jal x0, f30_7
f30_6:
    ld x5, 72(x2)
    ld x7, 0(x2)
    sd x5, 16(x7)
    jal x0, f30_8
f30_7:
    jal x0, f30_8
f30_8:
    addi x10, x0, 0
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
# pfa_free
fn_31:
    addi x2, x2, -128
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 120(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f31_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 32(x2)
    ld x5, 16(x2)
    ld x6, 32(x2)
    sltu x5, x5, x6
    xori x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f31_1
    jal x0, f31_2
f31_1:
    addi x10, x0, -2
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f31_2:
    jal x0, f31_3
f31_3:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 16(x2)
    jal x1, fn_9
    sb x10, 48(x2)
    jal x0, f31_5
f31_5:
    lbu x5, 48(x2)
    xori x5, x5, 1
    sb x5, 56(x2)
    lbu x5, 56(x2)
    beq x5, x0, 8
    jal x0, f31_6
    jal x0, f31_7
f31_6:
    addi x10, x0, -3
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
f31_7:
    jal x0, f31_8
f31_8:
    ld x10, 8(x2)
    ld x11, 16(x2)
    jal x1, fn_35
    jal x0, f31_10
f31_10:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 64(x2)
    ld x5, 64(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 72(x2)
    ld x5, 72(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    ld x5, 16(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 80(x2)
    ld x5, 80(x2)
    sd x5, 88(x2)
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 96(x2)
    ld x5, 88(x2)
    ld x6, 96(x2)
    sltu x5, x5, x6
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f31_11
    jal x0, f31_12
f31_11:
    ld x5, 88(x2)
    ld x7, 0(x2)
    sd x5, 16(x7)
    jal x0, f31_13
f31_12:
    jal x0, f31_13
f31_13:
    addi x10, x0, 0
    ld x1, 120(x2)
    addi x2, x2, 128
    jalr x0, 0(x1)
# ctl_alloc_run_off
fn_32:
    addi x2, x2, -208
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 200(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f32_0:
    ld x5, 16(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 32(x2)
    lbu x5, 32(x2)
    beq x5, x0, 8
    jal x0, f32_1
    jal x0, f32_2
f32_1:
    addi x10, x0, -2
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f32_2:
    jal x0, f32_3
f32_3:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 40(x2)
    ld x5, 16(x2)
    ld x6, 40(x2)
    sltu x5, x6, x5
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f32_5
    jal x0, f32_6
f32_5:
    addi x10, x0, -1
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f32_6:
    jal x0, f32_7
f32_7:
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 56(x2)
    ld x5, 56(x2)
    addi x6, x0, 64
    mulhu x7, x5, x6
    mul x5, x5, x6
    beq x7, x0, 8
    jal x0, rt_ovf_mul
    sd x5, 64(x2)
    ld x5, 64(x2)
    sd x5, 72(x2)
    ld x5, 72(x2)
    sd x5, 80(x2)
    addi x5, x0, 0
    sd x5, 88(x2)
    jal x0, f32_9
f32_9:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 96(x2)
    ld x5, 72(x2)
    ld x6, 96(x2)
    sltu x5, x5, x6
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f32_10
    jal x0, f32_11
f32_10:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 72(x2)
    jal x1, fn_9
    sb x10, 112(x2)
    jal x0, f32_12
f32_11:
    addi x10, x0, -1
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f32_12:
    lbu x5, 112(x2)
    beq x5, x0, 8
    jal x0, f32_13
    jal x0, f32_14
f32_13:
    addi x5, x0, 1
    sd x5, 88(x2)
    ld x5, 72(x2)
    sd x5, 80(x2)
    jal x0, f32_15
f32_14:
    ld x5, 88(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 120(x2)
    ld x5, 120(x2)
    sd x5, 88(x2)
    jal x0, f32_15
f32_15:
    ld x5, 88(x2)
    ld x6, 16(x2)
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 128(x2)
    lbu x5, 128(x2)
    beq x5, x0, 8
    jal x0, f32_16
    jal x0, f32_17
f32_16:
    ld x5, 80(x2)
    sd x5, 136(x2)
    jal x0, f32_19
f32_17:
    jal x0, f32_18
f32_18:
    ld x5, 72(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 184(x2)
    ld x5, 184(x2)
    sd x5, 72(x2)
    jal x0, f32_9
f32_19:
    ld x5, 136(x2)
    ld x6, 72(x2)
    sltu x5, x6, x5
    xori x5, x5, 1
    sb x5, 144(x2)
    lbu x5, 144(x2)
    beq x5, x0, 8
    jal x0, f32_20
    jal x0, f32_21
f32_20:
    ld x10, 8(x2)
    ld x11, 136(x2)
    jal x1, fn_28
    jal x0, f32_22
f32_21:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 160(x2)
    ld x5, 160(x2)
    ld x6, 16(x2)
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 168(x2)
    ld x5, 168(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    ld x5, 80(x2)
    sd x5, 176(x2)
    ld x10, 176(x2)
    ld x1, 200(x2)
    addi x2, x2, 208
    jalr x0, 0(x1)
f32_22:
    ld x5, 136(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 152(x2)
    ld x5, 152(x2)
    sd x5, 136(x2)
    jal x0, f32_19
# pfa_alloc_run
fn_33:
    addi x2, x2, -224
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 216(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f33_0:
    ld x5, 16(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 32(x2)
    lbu x5, 32(x2)
    beq x5, x0, 8
    jal x0, f33_1
    jal x0, f33_2
f33_1:
    addi x10, x0, -2
    ld x1, 216(x2)
    addi x2, x2, 224
    jalr x0, 0(x1)
f33_2:
    jal x0, f33_3
f33_3:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 40(x2)
    ld x5, 16(x2)
    ld x6, 40(x2)
    sltu x5, x6, x5
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f33_5
    jal x0, f33_6
f33_5:
    addi x10, x0, -1
    ld x1, 216(x2)
    addi x2, x2, 224
    jalr x0, 0(x1)
f33_6:
    jal x0, f33_7
f33_7:
    ld x7, 0(x2)
    addi x7, x7, 16
    ld x5, 0(x7)
    sd x5, 56(x2)
    ld x5, 56(x2)
    addi x6, x0, 64
    mulhu x7, x5, x6
    mul x5, x5, x6
    beq x7, x0, 8
    jal x0, rt_ovf_mul
    sd x5, 64(x2)
    ld x5, 64(x2)
    sd x5, 72(x2)
    ld x5, 72(x2)
    sd x5, 80(x2)
    addi x5, x0, 0
    sd x5, 88(x2)
    jal x0, f33_9
f33_9:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 96(x2)
    ld x5, 72(x2)
    ld x6, 96(x2)
    sltu x5, x5, x6
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f33_10
    jal x0, f33_11
f33_10:
    ld x10, 0(x2)
    ld x11, 8(x2)
    ld x12, 72(x2)
    jal x1, fn_9
    sb x10, 112(x2)
    jal x0, f33_12
f33_11:
    addi x10, x0, -1
    ld x1, 216(x2)
    addi x2, x2, 224
    jalr x0, 0(x1)
f33_12:
    lbu x5, 112(x2)
    beq x5, x0, 8
    jal x0, f33_13
    jal x0, f33_14
f33_13:
    addi x5, x0, 0
    sd x5, 88(x2)
    ld x5, 72(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 120(x2)
    ld x5, 120(x2)
    sd x5, 80(x2)
    jal x0, f33_15
f33_14:
    ld x5, 88(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 128(x2)
    ld x5, 128(x2)
    sd x5, 88(x2)
    ld x5, 88(x2)
    ld x6, 16(x2)
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 136(x2)
    lbu x5, 136(x2)
    beq x5, x0, 8
    jal x0, f33_16
    jal x0, f33_17
f33_15:
    ld x5, 72(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 192(x2)
    ld x5, 192(x2)
    sd x5, 72(x2)
    jal x0, f33_9
f33_16:
    ld x5, 80(x2)
    sd x5, 144(x2)
    jal x0, f33_19
f33_17:
    jal x0, f33_18
f33_18:
    jal x0, f33_15
f33_19:
    ld x5, 144(x2)
    ld x6, 72(x2)
    sltu x5, x6, x5
    xori x5, x5, 1
    sb x5, 152(x2)
    lbu x5, 152(x2)
    beq x5, x0, 8
    jal x0, f33_20
    jal x0, f33_21
f33_20:
    ld x10, 8(x2)
    ld x11, 144(x2)
    jal x1, fn_28
    jal x0, f33_22
f33_21:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 168(x2)
    ld x5, 168(x2)
    ld x6, 16(x2)
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 176(x2)
    ld x5, 176(x2)
    ld x7, 0(x2)
    sd x5, 8(x7)
    ld x5, 80(x2)
    sd x5, 184(x2)
    ld x10, 184(x2)
    ld x1, 216(x2)
    addi x2, x2, 224
    jalr x0, 0(x1)
f33_22:
    ld x5, 144(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 160(x2)
    ld x5, 160(x2)
    sd x5, 144(x2)
    jal x0, f33_19
# pfa_range_ok
fn_34:
    addi x2, x2, -112
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 104(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sd x12, 16(x2)
f34_0:
    ld x5, 16(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltu x5, x0, x5
    sb x5, 32(x2)
    lbu x5, 32(x2)
    beq x5, x0, 8
    jal x0, f34_1
    jal x0, f34_2
f34_1:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 48(x2)
    ld x5, 8(x2)
    ld x6, 48(x2)
    sltu x5, x5, x6
    sb x5, 56(x2)
    lbu x5, 56(x2)
    sb x5, 40(x2)
    jal x0, f34_3
f34_2:
    addi x5, x0, 0
    sb x5, 40(x2)
    jal x0, f34_3
f34_3:
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f34_4
    jal x0, f34_5
f34_4:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 72(x2)
    ld x5, 72(x2)
    ld x6, 8(x2)
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 80(x2)
    ld x5, 16(x2)
    ld x6, 80(x2)
    sltu x5, x6, x5
    xori x5, x5, 1
    sb x5, 88(x2)
    lbu x5, 88(x2)
    sb x5, 64(x2)
    jal x0, f34_6
f34_5:
    addi x5, x0, 0
    sb x5, 64(x2)
    jal x0, f34_6
f34_6:
    lbu x10, 64(x2)
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
# pfa_clear
fn_35:
    addi x2, x2, -96
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 88(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f35_0:
    ld x5, 8(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_div_zero
    divu x5, x5, x6
    sd x5, 16(x2)
    ld x5, 16(x2)
    sd x5, 24(x2)
    ld x5, 24(x2)
    sd x5, 32(x2)
    ld x28, 32(x2)
    ld x6, 0(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    ld x5, 0(x7)
    sd x5, 40(x2)
    ld x5, 8(x2)
    addi x6, x0, 64
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 48(x2)
    addi x5, x0, 1
    ld x6, 48(x2)
    andi x6, x6, 63
    sll x5, x5, x6
    sd x5, 56(x2)
    ld x5, 56(x2)
    xori x5, x5, -1
    sd x5, 64(x2)
    ld x5, 40(x2)
    ld x6, 64(x2)
    and x5, x5, x6
    sd x5, 72(x2)
    ld x5, 72(x2)
    ld x28, 32(x2)
    ld x6, 0(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 3
    add x7, x6, x29
    sd x5, 0(x7)
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
