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
    addi x10, x0, 3
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
# run
fn_0:
    addi x2, x2, -528
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 520(x2)
    sd x10, 40(x2)
f0_0:
    ld x5, 40(x2)
    addi x6, x0, 8
    sltu x5, x5, x6
    xori x5, x5, 1
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f0_1
    jal x0, f0_2
f0_1:
    ld x10, 40(x2)
    jal x1, fn_1
    jal x0, f0_4
f0_2:
    jal x0, f0_3
f0_3:
    ld x5, 40(x2)
    addi x6, x0, 10
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 56(x2)
    ld x10, 56(x2)
    jal x1, fn_2
    sd x10, 64(x2)
    jal x0, f0_6
f0_4:
    ld x1, 520(x2)
    addi x2, x2, 528
    jalr x0, 0(x1)
f0_6:
    addi x10, x0, 1
    jal x1, fn_2
    sd x10, 72(x2)
    jal x0, f0_7
f0_7:
    jal x5, 20
    .word 543320422
    .word 1970039845
    .word 1819026720
    .word 2677
    sd x5, 80(x2)
    jal x10, 12
    .word 543320422
    .word 0
    jal x1, rt_puts
    ld x10, 64(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    ld x10, 72(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_8
f0_8:
    ld x5, 40(x2)
    addi x6, x0, 20
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 88(x2)
    ld x10, 88(x2)
    jal x1, fn_3
    sb x10, 96(x2)
    jal x0, f0_9
f0_9:
    ld x5, 40(x2)
    addi x6, x0, 20
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 104(x2)
    ld x10, 104(x2)
    jal x1, fn_4
    sb x10, 112(x2)
    jal x0, f0_10
f0_10:
    jal x5, 16
    .word 1852143205
    .word 544417056
    .word 684837
    sd x5, 120(x2)
    jal x10, 12
    .word 1852143205
    .word 32
    jal x1, rt_puts
    lbu x5, 96(x2)
    jal x10, 12
    .word 1936482662
    .word 101
    beq x5, x0, 16
    jal x10, 12
    .word 1702195828
    .word 0
    jal x1, rt_puts
    jal x10, 8
    .word 32
    jal x1, rt_puts
    lbu x5, 112(x2)
    jal x10, 12
    .word 1936482662
    .word 101
    beq x5, x0, 16
    jal x10, 12
    .word 1702195828
    .word 0
    jal x1, rt_puts
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_11
f0_11:
    addi x10, x0, 2
    ld x11, 40(x2)
    jal x1, fn_5
    sd x10, 128(x2)
    jal x0, f0_12
f0_12:
    jal x5, 16
    .word 543908705
    .word 1970039845
    .word 10
    sd x5, 136(x2)
    jal x10, 12
    .word 543908705
    .word 0
    jal x1, rt_puts
    ld x10, 128(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_13
f0_13:
    addi x5, x0, 9
    sd x5, 0(x2)
    addi x5, x0, 10
    sd x5, 8(x2)
    addi x5, x0, 1
    sd x5, 16(x2)
    addi x5, x0, 127
    sd x5, 24(x2)
    addi x10, x0, 255
    addi x11, x0, -127
    jal x12, 12
    .word 65535
    .word 0
    ld x12, 0(x12)
    jal x13, 12
    .word 4294934529
    .word 4294967295
    ld x13, 0(x13)
    jal x14, 12
    .word 4294967295
    .word 0
    ld x14, 0(x14)
    jal x15, 12
    .word 2147483649
    .word 4294967295
    ld x15, 0(x15)
    ld x16, 40(x2)
    addi x17, x0, -8
    jal x1, fn_6
    sd x10, 144(x2)
    jal x0, f0_14
f0_14:
    jal x5, 16
    .word 2037277037
    .word 1819026720
    .word 2660
    sd x5, 152(x2)
    jal x10, 12
    .word 2037277037
    .word 32
    jal x1, rt_puts
    ld x10, 144(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_15
f0_15:
    addi x5, x0, 100
    sd x5, 160(x2)
    addi x5, x0, -50
    sd x5, 168(x2)
    addi x6, x2, 176
    addi x7, x2, 160
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    addi x5, x0, 1000
    sd x5, 192(x2)
    addi x5, x0, 7
    sd x5, 200(x2)
    addi x6, x2, 208
    addi x7, x2, 192
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    addi x7, x2, 208
    addi x5, x7, 0
    sd x5, 224(x2)
    ld x5, 40(x2)
    sd x5, 0(x2)
    addi x5, x2, 176
    sd x5, 8(x2)
    ld x5, 224(x2)
    sd x5, 16(x2)
    jal x5, 12
    .word 60000
    .word 0
    ld x5, 0(x5)
    sd x5, 24(x2)
    addi x11, x0, 1
    addi x12, x0, 2
    addi x13, x0, 3
    addi x14, x0, 4
    addi x15, x0, 5
    addi x16, x0, 6
    addi x17, x0, 7
    addi x10, x2, 232
    jal x1, fn_7
    jal x0, f0_16
f0_16:
    addi x6, x2, 248
    addi x7, x2, 232
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    addi x7, x2, 248
    ld x5, 0(x7)
    sd x5, 264(x2)
    addi x7, x2, 256
    ld x5, 0(x7)
    sd x5, 272(x2)
    jal x5, 20
    .word 1701079415
    .word 1819026720
    .word 1814372469
    .word 681068
    sd x5, 280(x2)
    jal x10, 12
    .word 1701079415
    .word 32
    jal x1, rt_puts
    ld x10, 264(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    ld x10, 272(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_17
f0_17:
    addi x5, x0, 41
    sd x5, 288(x2)
    addi x7, x2, 288
    addi x5, x7, 0
    sd x5, 296(x2)
    ld x10, 296(x2)
    ld x11, 40(x2)
    jal x1, fn_8
    jal x0, f0_18
f0_18:
    addi x5, x0, -127
    sb x5, 304(x2)
    addi x7, x2, 304
    addi x5, x7, 0
    sd x5, 312(x2)
    ld x10, 312(x2)
    jal x1, fn_9
    jal x0, f0_19
f0_19:
    jal x5, 20
    .word 1936090482
    .word 1819026720
    .word 1680154741
    .word 10
    sd x5, 320(x2)
    jal x10, 12
    .word 1936090482
    .word 32
    jal x1, rt_puts
    ld x10, 288(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    lb x10, 304(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_20
f0_20:
    ld x10, 40(x2)
    jal x1, fn_10
    sd x10, 328(x2)
    jal x0, f0_21
f0_21:
    addi x10, x0, 7
    jal x1, fn_10
    sd x10, 336(x2)
    jal x0, f0_22
f0_22:
    addi x10, x0, 9
    jal x1, fn_10
    sd x10, 344(x2)
    jal x0, f0_23
f0_23:
    jal x5, 28
    .word 1801677168
    .word 1819026720
    .word 1814372452
    .word 622879852
    .word 174353516
    .word 0
    sd x5, 352(x2)
    jal x10, 12
    .word 1801677168
    .word 32
    jal x1, rt_puts
    ld x10, 328(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    ld x10, 336(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    ld x10, 344(x2)
    jal x1, rt_put_i64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_24
f0_24:
    jal x5, 12
    .word 4000000000
    .word 0
    ld x5, 0(x5)
    sw x5, 360(x2)
    addi x5, x0, 2
    sw x5, 364(x2)
    addi x5, x0, 3
    sw x5, 368(x2)
    addi x5, x0, 4
    sw x5, 372(x2)
    addi x5, x0, 5
    sw x5, 376(x2)
    addi x6, x2, 384
    addi x7, x2, 360
    addi x29, x7, 0
    addi x30, x6, 0
    addi x28, x0, 20
    lbu x31, 0(x29)
    sb x31, 0(x30)
    addi x29, x29, 1
    addi x30, x30, 1
    addi x28, x28, -1
    bne x28, x0, -20
    addi x5, x2, 384
    sd x5, 408(x2)
    addi x5, x0, 5
    sd x5, 416(x2)
    addi x7, x2, 408
    addi x5, x7, 0
    sd x5, 424(x2)
    ld x5, 40(x2)
    addi x6, x0, 5
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 432(x2)
    ld x10, 424(x2)
    ld x11, 432(x2)
    addi x12, x0, 77
    jal x1, fn_11
    jal x0, f0_25
f0_25:
    addi x5, x2, 384
    sd x5, 440(x2)
    addi x5, x0, 5
    sd x5, 448(x2)
    addi x7, x2, 440
    addi x5, x7, 0
    sd x5, 456(x2)
    ld x10, 456(x2)
    jal x1, fn_12
    sd x10, 464(x2)
    jal x0, f0_26
f0_26:
    ld x5, 40(x2)
    addi x6, x0, 5
    bne x6, x0, 8
    jal x0, rt_rem_zero
    remu x5, x5, x6
    sd x5, 472(x2)
    ld x5, 472(x2)
    sd x5, 480(x2)
    ld x28, 480(x2)
    addi x7, x0, 5
    addi x6, x2, 384
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 2
    add x7, x6, x29
    lwu x5, 0(x7)
    sw x5, 488(x2)
    jal x5, 20
    .word 1667853427
    .word 1814372453
    .word 622884204
    .word 2677
    sd x5, 496(x2)
    jal x10, 12
    .word 1667853427
    .word 8293
    jal x1, rt_puts
    ld x10, 464(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 32
    jal x1, rt_puts
    lwu x10, 488(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f0_27
f0_27:
    ld x1, 520(x2)
    addi x2, x2, 528
    jalr x0, 0(x1)
# traps
fn_1:
    addi x2, x2, -320
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 312(x2)
    sd x10, 0(x2)
f1_0:
    addi x5, x0, 1
    sw x5, 8(x2)
    addi x5, x0, 2
    sw x5, 12(x2)
    addi x5, x0, 3
    sw x5, 16(x2)
    addi x5, x0, 4
    sw x5, 20(x2)
    addi x6, x2, 24
    addi x7, x2, 8
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    jal x1, rt_vec_new
    sd x10, 40(x2)
    jal x0, f1_1
f1_1:
    ld x5, 40(x2)
    sd x5, 48(x2)
    ld x10, 48(x2)
    addi x11, x0, 1
    addi x12, x0, 1
    jal x1, rt_vec_push
    jal x0, f1_2
f1_2:
    ld x10, 48(x2)
    addi x11, x0, 2
    addi x12, x0, 1
    jal x1, rt_vec_push
    jal x0, f1_3
f1_3:
    jal x5, 20
    .word 1885434484
    .word 1935762208
    .word 1814372453
    .word 685420
    sd x5, 56(x2)
    jal x10, 16
    .word 1885434484
    .word 1935762208
    .word 8293
    jal x1, rt_puts
    ld x10, 0(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f1_4
f1_4:
    ld x5, 0(x2)
    addi x6, x0, 8
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 64(x2)
    lbu x5, 64(x2)
    beq x5, x0, 8
    jal x0, f1_5
    jal x0, f1_6
f1_5:
    ld x5, 0(x2)
    addi x6, x0, 4
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 72(x2)
    ld x5, 72(x2)
    sd x5, 80(x2)
    ld x28, 80(x2)
    addi x7, x0, 4
    addi x6, x2, 24
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 2
    add x7, x6, x29
    lwu x5, 0(x7)
    sw x5, 88(x2)
    jal x5, 8
    .word 685349
    sd x5, 96(x2)
    lwu x10, 88(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f1_8
f1_6:
    jal x0, f1_7
f1_7:
    ld x5, 0(x2)
    addi x6, x0, 9
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f1_9
    jal x0, f1_10
f1_8:
    jal x0, f1_7
f1_9:
    addi x5, x2, 24
    sd x5, 112(x2)
    addi x5, x0, 4
    sd x5, 120(x2)
    addi x7, x2, 112
    addi x5, x7, 0
    sd x5, 128(x2)
    ld x10, 128(x2)
    addi x11, x0, 4
    addi x12, x0, 9
    jal x1, fn_11
    jal x0, f1_12
f1_10:
    jal x0, f1_11
f1_11:
    ld x5, 0(x2)
    addi x6, x0, 10
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 136(x2)
    lbu x5, 136(x2)
    beq x5, x0, 8
    jal x0, f1_13
    jal x0, f1_14
f1_12:
    jal x0, f1_11
f1_13:
    addi x5, x2, 24
    sd x5, 144(x2)
    addi x5, x0, 4
    sd x5, 152(x2)
    addi x7, x2, 144
    addi x5, x7, 0
    sd x5, 160(x2)
    ld x10, 160(x2)
    addi x11, x0, -1
    addi x12, x0, 9
    jal x1, fn_11
    jal x0, f1_16
f1_14:
    jal x0, f1_15
f1_15:
    ld x5, 0(x2)
    addi x6, x0, 11
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 168(x2)
    lbu x5, 168(x2)
    beq x5, x0, 8
    jal x0, f1_17
    jal x0, f1_18
f1_16:
    jal x0, f1_15
f1_17:
    addi x28, x0, 2
    ld x6, 48(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    add x7, x6, x28
    lbu x5, 0(x7)
    sb x5, 176(x2)
    jal x5, 8
    .word 685349
    sd x5, 184(x2)
    lbu x10, 176(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f1_20
f1_18:
    jal x0, f1_19
f1_19:
    ld x5, 0(x2)
    addi x6, x0, 12
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 192(x2)
    lbu x5, 192(x2)
    beq x5, x0, 8
    jal x0, f1_21
    jal x0, f1_22
f1_20:
    jal x0, f1_19
f1_21:
    addi x5, x0, 1
    addi x28, x0, 5
    ld x6, 48(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x28, 0
    addi x11, x7, 0
    jal x0, rt_vec_oob
    add x7, x6, x28
    sb x5, 0(x7)
    jal x0, f1_24
f1_22:
    jal x0, f1_23
f1_23:
    ld x5, 0(x2)
    addi x6, x0, 13
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 200(x2)
    lbu x5, 200(x2)
    beq x5, x0, 8
    jal x0, f1_25
    jal x0, f1_26
f1_24:
    jal x0, f1_23
f1_25:
    ld x5, 0(x2)
    addi x6, x0, 13
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 208(x2)
    addi x5, x0, 0
    ld x6, 208(x2)
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 216(x2)
    ld x5, 216(x2)
    sd x5, 224(x2)
    ld x5, 224(x2)
    addi x6, x0, 1000
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 232(x2)
    ld x28, 232(x2)
    addi x7, x0, 4
    addi x6, x2, 24
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 2
    add x7, x6, x29
    lwu x5, 0(x7)
    sw x5, 240(x2)
    jal x5, 8
    .word 685349
    sd x5, 248(x2)
    lwu x10, 240(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f1_28
f1_26:
    jal x0, f1_27
f1_27:
    addi x5, x2, 24
    sd x5, 256(x2)
    addi x5, x0, 4
    sd x5, 264(x2)
    addi x7, x2, 256
    addi x5, x7, 0
    sd x5, 272(x2)
    ld x10, 272(x2)
    jal x1, fn_12
    sd x10, 280(x2)
    jal x0, f1_29
f1_28:
    jal x0, f1_27
f1_29:
    jal x5, 20
    .word 1948282734
    .word 544235890
    .word 1970039845
    .word 10
    sd x5, 288(x2)
    jal x10, 16
    .word 1948282734
    .word 544235890
    .word 0
    jal x1, rt_puts
    ld x10, 280(x2)
    jal x1, rt_put_u64
    jal x10, 8
    .word 10
    jal x1, rt_puts
    jal x0, f1_30
f1_30:
    ld x1, 312(x2)
    addi x2, x2, 320
    jalr x0, 0(x1)
# fib
fn_2:
    addi x2, x2, -80
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 72(x2)
    sd x10, 0(x2)
f2_0:
    ld x5, 0(x2)
    addi x6, x0, 2
    sltu x5, x5, x6
    sb x5, 16(x2)
    lbu x5, 16(x2)
    beq x5, x0, 8
    jal x0, f2_1
    jal x0, f2_2
f2_1:
    ld x10, 0(x2)
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f2_2:
    jal x0, f2_3
f2_3:
    ld x5, 0(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 24(x2)
    ld x10, 24(x2)
    jal x1, fn_2
    sd x10, 32(x2)
    jal x0, f2_5
f2_5:
    ld x5, 0(x2)
    addi x6, x0, 2
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 40(x2)
    ld x10, 40(x2)
    jal x1, fn_2
    sd x10, 48(x2)
    jal x0, f2_6
f2_6:
    ld x5, 32(x2)
    ld x6, 48(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 56(x2)
    ld x10, 56(x2)
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
# is_even
fn_3:
    addi x2, x2, -64
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 56(x2)
    sd x10, 0(x2)
f3_0:
    ld x5, 0(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 16(x2)
    lbu x5, 16(x2)
    beq x5, x0, 8
    jal x0, f3_1
    jal x0, f3_2
f3_1:
    addi x10, x0, 1
    ld x1, 56(x2)
    addi x2, x2, 64
    jalr x0, 0(x1)
f3_2:
    jal x0, f3_3
f3_3:
    ld x5, 0(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 24(x2)
    ld x10, 24(x2)
    jal x1, fn_4
    sb x10, 32(x2)
    jal x0, f3_5
f3_5:
    lbu x10, 32(x2)
    ld x1, 56(x2)
    addi x2, x2, 64
    jalr x0, 0(x1)
# is_odd
fn_4:
    addi x2, x2, -64
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 56(x2)
    sd x10, 0(x2)
f4_0:
    ld x5, 0(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 16(x2)
    lbu x5, 16(x2)
    beq x5, x0, 8
    jal x0, f4_1
    jal x0, f4_2
f4_1:
    addi x10, x0, 0
    ld x1, 56(x2)
    addi x2, x2, 64
    jalr x0, 0(x1)
f4_2:
    jal x0, f4_3
f4_3:
    ld x5, 0(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 24(x2)
    ld x10, 24(x2)
    jal x1, fn_3
    sb x10, 32(x2)
    jal x0, f4_5
f4_5:
    lbu x10, 32(x2)
    ld x1, 56(x2)
    addi x2, x2, 64
    jalr x0, 0(x1)
# ack
fn_5:
    addi x2, x2, -112
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 104(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f5_0:
    ld x5, 0(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 24(x2)
    lbu x5, 24(x2)
    beq x5, x0, 8
    jal x0, f5_1
    jal x0, f5_2
f5_1:
    ld x5, 8(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 32(x2)
    ld x10, 32(x2)
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
f5_2:
    jal x0, f5_3
f5_3:
    ld x5, 8(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f5_5
    jal x0, f5_6
f5_5:
    ld x5, 0(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 48(x2)
    ld x10, 48(x2)
    addi x11, x0, 1
    jal x1, fn_5
    sd x10, 56(x2)
    jal x0, f5_8
f5_6:
    jal x0, f5_7
f5_7:
    ld x5, 0(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 64(x2)
    ld x5, 8(x2)
    addi x6, x0, 1
    sub x7, x5, x6
    bgeu x5, x6, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 72(x2)
    ld x10, 0(x2)
    ld x11, 72(x2)
    jal x1, fn_5
    sd x10, 80(x2)
    jal x0, f5_10
f5_8:
    ld x10, 56(x2)
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
f5_10:
    ld x10, 64(x2)
    ld x11, 80(x2)
    jal x1, fn_5
    sd x10, 88(x2)
    jal x0, f5_11
f5_11:
    ld x10, 88(x2)
    ld x1, 104(x2)
    addi x2, x2, 112
    jalr x0, 0(x1)
# many
fn_6:
    addi x2, x2, -368
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 360(x2)
    sb x10, 0(x2)
    sb x11, 8(x2)
    sh x12, 16(x2)
    sh x13, 24(x2)
    sw x14, 32(x2)
    sw x15, 40(x2)
    sd x16, 48(x2)
    sd x17, 56(x2)
    ld x5, 368(x2)
    sb x5, 64(x2)
    ld x5, 376(x2)
    sd x5, 72(x2)
    ld x5, 384(x2)
    sb x5, 80(x2)
    ld x5, 392(x2)
    sb x5, 88(x2)
f6_0:
    lbu x5, 80(x2)
    xori x5, x5, 1
    sb x5, 104(x2)
    lbu x5, 104(x2)
    beq x5, x0, 8
    jal x0, f6_1
    jal x0, f6_2
f6_1:
    addi x10, x0, 0
    ld x1, 360(x2)
    addi x2, x2, 368
    jalr x0, 0(x1)
f6_2:
    jal x0, f6_3
f6_3:
    lbu x5, 0(x2)
    sd x5, 112(x2)
    lb x5, 8(x2)
    sd x5, 120(x2)
    addi x5, x0, 2
    ld x6, 120(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 128(x2)
    ld x5, 112(x2)
    ld x6, 128(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 136(x2)
    lhu x5, 16(x2)
    sd x5, 144(x2)
    addi x5, x0, 3
    ld x6, 144(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 152(x2)
    ld x5, 136(x2)
    ld x6, 152(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 160(x2)
    lh x5, 24(x2)
    sd x5, 168(x2)
    addi x5, x0, 4
    ld x6, 168(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 176(x2)
    ld x5, 160(x2)
    ld x6, 176(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 184(x2)
    lwu x5, 32(x2)
    sd x5, 192(x2)
    addi x5, x0, 5
    ld x6, 192(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 200(x2)
    ld x5, 184(x2)
    ld x6, 200(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 208(x2)
    lw x5, 40(x2)
    sd x5, 216(x2)
    addi x5, x0, 6
    ld x6, 216(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 224(x2)
    ld x5, 208(x2)
    ld x6, 224(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 232(x2)
    ld x5, 48(x2)
    sd x5, 240(x2)
    addi x5, x0, 7
    ld x6, 240(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 248(x2)
    ld x5, 232(x2)
    ld x6, 248(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 256(x2)
    addi x5, x0, 8
    ld x6, 56(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 264(x2)
    ld x5, 256(x2)
    ld x6, 264(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 272(x2)
    lbu x5, 64(x2)
    sd x5, 280(x2)
    addi x5, x0, 9
    ld x6, 280(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 288(x2)
    ld x5, 272(x2)
    ld x6, 288(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 296(x2)
    addi x5, x0, 10
    ld x6, 72(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 304(x2)
    ld x5, 296(x2)
    ld x6, 304(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 312(x2)
    lb x5, 88(x2)
    sd x5, 320(x2)
    addi x5, x0, 11
    ld x6, 320(x2)
    mulh x7, x5, x6
    mul x5, x5, x6
    srai x28, x5, 63
    beq x7, x28, 8
    jal x0, rt_ovf_mul
    sd x5, 328(x2)
    ld x5, 312(x2)
    ld x6, 328(x2)
    add x7, x5, x6
    slti x28, x6, 0
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 336(x2)
    ld x10, 336(x2)
    ld x1, 360(x2)
    addi x2, x2, 368
    jalr x0, 0(x1)
# wide
fn_7:
    addi x2, x2, -272
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 264(x2)
    sd x10, 256(x2)
    sd x11, 0(x2)
    sd x12, 8(x2)
    sd x13, 16(x2)
    sd x14, 24(x2)
    sd x15, 32(x2)
    sd x16, 40(x2)
    sd x17, 48(x2)
    ld x5, 272(x2)
    sd x5, 56(x2)
    ld x5, 280(x2)
    addi x7, x5, 0
    addi x6, x2, 64
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x5, 288(x2)
    sd x5, 80(x2)
    ld x5, 296(x2)
    sh x5, 88(x2)
f7_0:
    ld x5, 0(x2)
    ld x6, 8(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 112(x2)
    ld x5, 112(x2)
    ld x6, 16(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 120(x2)
    ld x5, 120(x2)
    ld x6, 24(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 128(x2)
    ld x5, 128(x2)
    ld x6, 32(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 136(x2)
    ld x5, 136(x2)
    ld x6, 40(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 144(x2)
    ld x5, 144(x2)
    ld x6, 48(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 152(x2)
    ld x5, 152(x2)
    ld x6, 56(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 160(x2)
    addi x7, x2, 64
    ld x5, 0(x7)
    sd x5, 168(x2)
    ld x5, 160(x2)
    ld x6, 168(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 176(x2)
    ld x7, 80(x2)
    ld x5, 0(x7)
    sd x5, 184(x2)
    ld x5, 176(x2)
    ld x6, 184(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 192(x2)
    lhu x5, 88(x2)
    sd x5, 200(x2)
    ld x5, 192(x2)
    ld x6, 200(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 208(x2)
    addi x7, x2, 72
    ld x5, 0(x7)
    sd x5, 216(x2)
    ld x7, 80(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 224(x2)
    ld x5, 216(x2)
    ld x6, 224(x2)
    sub x7, x5, x6
    slt x28, x0, x6
    slt x29, x7, x5
    beq x28, x29, 8
    jal x0, rt_ovf_sub
    addi x5, x7, 0
    sd x5, 232(x2)
    ld x5, 208(x2)
    sd x5, 240(x2)
    ld x5, 232(x2)
    sd x5, 248(x2)
    ld x6, 256(x2)
    addi x7, x2, 240
    ld x28, 0(x7)
    sd x28, 0(x6)
    ld x28, 8(x7)
    sd x28, 8(x6)
    ld x1, 264(x2)
    addi x2, x2, 272
    jalr x0, 0(x1)
# bump
fn_8:
    addi x2, x2, -48
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 40(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
f8_0:
    ld x7, 0(x2)
    ld x5, 0(x7)
    sd x5, 16(x2)
    ld x5, 16(x2)
    ld x6, 8(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 24(x2)
    ld x5, 24(x2)
    ld x7, 0(x2)
    sd x5, 0(x7)
    ld x1, 40(x2)
    addi x2, x2, 48
    jalr x0, 0(x1)
# low
fn_9:
    addi x2, x2, -48
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 40(x2)
    sd x10, 0(x2)
f9_0:
    ld x7, 0(x2)
    lb x5, 0(x7)
    sb x5, 8(x2)
    lb x5, 8(x2)
    addi x6, x0, 1
    sub x5, x5, x6
    addi x7, x5, 0
    slli x5, x5, 56
    srai x5, x5, 56
    beq x5, x7, 8
    jal x0, rt_ovf_sub
    sb x5, 16(x2)
    lb x5, 16(x2)
    ld x7, 0(x2)
    sb x5, 0(x7)
    ld x1, 40(x2)
    addi x2, x2, 48
    jalr x0, 0(x1)
# pick
fn_10:
    addi x2, x2, -80
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 72(x2)
    sd x10, 0(x2)
f10_0:
    ld x5, 0(x2)
    sd x5, 16(x2)
    jal x0, f10_3
f10_1:
    ld x10, 24(x2)
    ld x1, 72(x2)
    addi x2, x2, 80
    jalr x0, 0(x1)
f10_2:
    addi x5, x0, 10
    sd x5, 24(x2)
    jal x0, f10_1
f10_3:
    ld x5, 16(x2)
    addi x6, x0, 0
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 32(x2)
    lbu x5, 32(x2)
    beq x5, x0, 8
    jal x0, f10_2
    jal x0, f10_5
f10_4:
    addi x5, x0, 20
    sd x5, 24(x2)
    jal x0, f10_1
f10_5:
    ld x5, 16(x2)
    addi x6, x0, 1
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f10_4
    jal x0, f10_7
f10_6:
    addi x5, x0, 70
    sd x5, 24(x2)
    jal x0, f10_1
f10_7:
    ld x5, 16(x2)
    addi x6, x0, 7
    xor x5, x5, x6
    sltiu x5, x5, 1
    sb x5, 48(x2)
    lbu x5, 48(x2)
    beq x5, x0, 8
    jal x0, f10_6
    jal x0, f10_8
f10_8:
    ld x5, 16(x2)
    sd x5, 56(x2)
    addi x5, x0, -1
    sd x5, 24(x2)
    jal x0, f10_1
# fill
fn_11:
    addi x2, x2, -48
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 40(x2)
    sd x10, 0(x2)
    sd x11, 8(x2)
    sw x12, 16(x2)
f11_0:
    ld x5, 8(x2)
    sd x5, 24(x2)
    lwu x5, 16(x2)
    ld x28, 24(x2)
    ld x6, 0(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 2
    add x7, x6, x29
    sw x5, 0(x7)
    ld x1, 40(x2)
    addi x2, x2, 48
    jalr x0, 0(x1)
# total
fn_12:
    addi x2, x2, -96
    bgeu x2, x27, 8
    jal x0, rt_stack
    sd x1, 88(x2)
    sd x10, 0(x2)
f12_0:
    addi x5, x0, 0
    sd x5, 16(x2)
    addi x5, x0, 0
    sd x5, 24(x2)
    jal x0, f12_1
f12_1:
    ld x7, 0(x2)
    addi x7, x7, 8
    ld x5, 0(x7)
    sd x5, 32(x2)
    ld x5, 24(x2)
    ld x6, 32(x2)
    sltu x5, x5, x6
    sb x5, 40(x2)
    lbu x5, 40(x2)
    beq x5, x0, 8
    jal x0, f12_2
    jal x0, f12_3
f12_2:
    ld x28, 24(x2)
    ld x6, 0(x2)
    ld x7, 8(x6)
    ld x6, 0(x6)
    bltu x28, x7, 16
    addi x10, x7, 0
    addi x11, x28, 0
    jal x0, rt_oob
    slli x29, x28, 2
    add x7, x6, x29
    lwu x5, 0(x7)
    sw x5, 48(x2)
    lwu x5, 48(x2)
    sd x5, 56(x2)
    ld x5, 16(x2)
    ld x6, 56(x2)
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 64(x2)
    ld x5, 64(x2)
    sd x5, 16(x2)
    ld x5, 24(x2)
    addi x6, x0, 1
    add x7, x5, x6
    bgeu x7, x5, 8
    jal x0, rt_ovf_add
    addi x5, x7, 0
    sd x5, 72(x2)
    ld x5, 72(x2)
    sd x5, 24(x2)
    jal x0, f12_1
f12_3:
    ld x10, 16(x2)
    ld x1, 88(x2)
    addi x2, x2, 96
    jalr x0, 0(x1)
