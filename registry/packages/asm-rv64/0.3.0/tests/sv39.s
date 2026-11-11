# sv39.s: a program for the system profile of emu-rv64 0.6.0. Machine code
# builds a page table and delegates two causes, then drops to supervisor
# mode. The supervisor turns paging on and enters user mode. The user
# program makes an environment call, stores and loads in its own page, and
# then loads from a page that is not mapped. The supervisor handles both
# traps and stops in a loop.
#
# Run with `emu-rv64 srun tests/sv39.img 200`, in 65536 bytes of memory.
#
# Physical memory, in pages of 4096 bytes from 0x80000000:
#   page 0  this code up to `mfail`     page 3  the level-1 table
#   page 1  the user program            page 4  the level-0 table
#   page 2  the root table              page 5  user data
# The tables map each page where it lies, so an address means the same
# before and after paging is on:
#   root[2]   -> level-1 table            (addresses 0x80000000 to 0xBFFFFFFF)
#   level1[0] -> level-0 table            (0x80000000 to 0x801FFFFF)
#   level0[0] =  page 0, flags 0x4B: V R X A      supervisor code
#   level0[1] =  page 1, flags 0x5B: V R X U A    user code
#   level0[5] =  page 5, flags 0xD7: V R W U A D  user data
# Page 6 has no entry. A page table entry is the page address shifted right
# by 2, which puts the page number at bit 10, with the flags in bits 7 to 0.
#
# Expected, worked out by hand. Instructions retired before the loop:
#   39 in M-mode (0x00 to 0x98), 15 in S-mode (0x9C to 0xD4), 1 in U-mode,
#   8 in the handler for the call, 5 in U-mode, 5 in the handler for the
#   fault: 73, with 2 traps, so the loop at `done` starts at step 76.
# After 200 steps: 198 retired, 2 traps, pc = 0x80000100, mode S (1),
#   scause = 13 (load page fault), stval = 0x80006018, sepc = 0x8000101C,
#   x10 = 42, x11 = 7, x12 = 7, x13 = 0x80006018, x14 = 42 (the argument
#   the handler saw), x15 = 0x80006018, x16 = 13, x17 = 0x8000101C.

        auipc   x18, 0              # 00: x18 = 0x80000000, the start of memory
        lui     x7, 1               # 04: x7 = 0x1000, one page
        add     x5, x18, x7         # 08
        add     x5, x5, x7          # 0C: x5 = 0x80002000, the root table
        add     x8, x5, x7          # 10: x8 = 0x80003000, the level-1 table
        add     x9, x8, x7          # 14: x9 = 0x80004000, the level-0 table
        srli    x6, x8, 2           # 18
        ori     x6, x6, 1           # 1C: a pointer: V alone
        sd      x6, 16(x5)          # 20: root[2] -> level-1 table
        srli    x6, x9, 2           # 24
        ori     x6, x6, 1           # 28
        sd      x6, 0(x8)           # 2C: level1[0] -> level-0 table
        srli    x6, x18, 2          # 30
        ori     x6, x6, 0x4B        # 34
        sd      x6, 0(x9)           # 38: level0[0] = page 0, supervisor code
        add     x6, x18, x7         # 3C: 0x80001000
        srli    x6, x6, 2           # 40
        ori     x6, x6, 0x5B        # 44
        sd      x6, 8(x9)           # 48: level0[1] = page 1, user code
        add     x6, x9, x7          # 4C: 0x80005000
        srli    x6, x6, 2           # 50
        ori     x6, x6, 0xD7        # 54
        sd      x6, 40(x9)          # 58: level0[5] = page 5, user data
        lui     x6, 2               # 5C: 0x2000, cause 13
        addi    x6, x6, 0x100       # 60: 0x2100, causes 13 and 8
        csrrw   x0, medeleg, x6     # 64: both go to the supervisor
        auipc   x6, 0               # 68: 0x80000068
        addi    x6, x6, 0x9C        # 6C: mfail, 0x104
        csrrw   x0, mtvec, x6       # 70
        lui     x6, 2               # 74: 0x2000
        addi    x6, x6, -2048       # 78: 0x1800, both bits of MPP
        csrrc   x0, mstatus, x6     # 7C
        lui     x6, 1               # 80: 0x1000
        addi    x6, x6, -2048       # 84: 0x800, MPP = S
        csrrs   x0, mstatus, x6     # 88
        auipc   x6, 0               # 8C: 0x8000008C
        addi    x6, x6, 0x10        # 90: s_entry, 0x9C
        csrrw   x0, mepc, x6        # 94
        mret                        # 98: to supervisor mode

s_entry:
        auipc   x6, 0               # 9C: 0x8000009C
        addi    x6, x6, 0x3C        # A0: s_trap, 0xD8
        csrrw   x0, stvec, x6       # A4
        addi    x6, x0, 1           # A8
        slli    x6, x6, 63          # AC: MODE 8, Sv39, is bit 63
        srli    x7, x5, 12          # B0: the page number of the root table
        or      x6, x6, x7          # B4
        csrrw   x0, satp, x6        # B8: paging is on from the next fetch
        sfence.vma x0, x0           # BC
        lui     x7, 1               # C0
        add     x6, x18, x7         # C4: 0x80001000, the user program
        csrrw   x0, sepc, x6        # C8
        addi    x6, x0, 0x100       # CC: SPP
        csrrc   x0, sstatus, x6     # D0: SPP = 0, user mode
        sret                        # D4: to user mode

s_trap:
        csrrs   x16, scause, x0     # D8
        addi    x6, x0, 8           # DC: environment call from U-mode
        bne     x16, x6, s_fault    # E0
        addi    x14, x10, 0         # E4: the argument of the call
        csrrs   x6, sepc, x0        # E8
        addi    x6, x6, 4           # EC: step over the ecall
        csrrw   x0, sepc, x6        # F0
        sret                        # F4

s_fault:
        csrrs   x15, stval, x0      # F8: the address that faulted
        csrrs   x17, sepc, x0       # FC: the instruction that faulted
done:   jal     x0, done            # 100: the supervisor stops here
mfail:  jal     x0, mfail           # 104: a trap that reached M-mode

        .org    0x1000
user:
        addi    x10, x0, 42         # 1000
        ecall                       # 1004: cause 8, handled by s_trap
        auipc   x19, 4              # 1008: x19 = 0x80005008, in user data
        addi    x11, x0, 7          # 100C
        sd      x11, 0(x19)         # 1010
        ld      x12, 0(x19)         # 1014: x12 = 7
        auipc   x13, 5              # 1018: x13 = 0x80006018, not mapped
        ld      x10, 0(x13)         # 101C: cause 13, load page fault
        jal     x0, user            # 1020: never reached
