# emu-rv64 0.6.0: RV64IMA machine emulator core with supervisor and user modes, Sv39 virtual memory, a timer, interrupts, flat images and replay

An emulator is a program that behaves like a computer: it reads the machine
code of that computer and carries out each instruction. This package is the
core of one for RISC-V, an open instruction set. It covers RV64I, the 64-bit
base integer set: 52 instructions, 32 registers of 64 bits, and a program
counter (`pc`), the address of the next instruction.

Version 0.2.0 adds what a machine needs to take a trap. A trap is a forced
jump to a handler, caused by the instruction that is running: an illegal
word, an environment call, a breakpoint, or an address outside memory.

- Control and status registers (CSRs): registers beside `x0` to `x31` that
  hold the state of the processor itself. Each has a 12-bit number.
- Zicsr: the six instructions that read and write CSRs.
- Machine mode: the highest privilege level of RISC-V, and the only one this
  machine has.
- MRET: the instruction that returns from a trap handler.

Version 0.3.0 adds what a machine needs to be interrupted. An interrupt is
a trap whose cause lies outside the instruction stream: a device asks for
attention, and the machine enters the handler in place of running the next
instruction.

- A timer device: two 64-bit registers in the address space, `mtime`, which
  counts, and `mtimecmp`, the value software compares it with.
- Three machine interrupts: timer, software and external.
- WFI, "wait for interrupt": an instruction a program runs when it has
  nothing to do.

Version 0.4.0 adds what a machine needs to run a program from a file, and
a way to show that a run repeats.

- A flat image: a file of raw bytes with no header, copied into memory at
  the address where the machine starts.
- A state hash: one 64-bit number computed from the whole state of a run.
  Equal states give equal numbers.
- A checkpoint: the state hash recorded at a fixed step of a run.
- Replay: the same image runs twice from scratch, and the checkpoints of
  the two runs are compared.

Version 0.5.0 adds two extensions of the instruction set, in the trap
profile:

- M, integer multiply and divide: 13 instructions.
- A, the atomic instructions: 22 instructions. LR and SC are a pair, "load
  reserved" and "store conditional": the store happens only if the mark the
  load left is still there. The nine AMOs, "atomic memory operations", each
  read a value, combine it with a register and store the result in one
  instruction.

Version 0.6.0 adds what a machine needs to run an operating system kernel
that keeps programs apart, in a third profile, the system profile:

- Two more privilege modes, supervisor (S) for a kernel and user (U) for
  the programs it runs, beside machine mode (M).
- The supervisor CSRs, and delegation: a trap can enter the supervisor
  handler directly.
- Sv39 virtual memory: each address a program uses is translated through
  three levels of page tables, which also say what the program may do
  with the page.
- SRET, the return from a supervisor handler, and SFENCE.VMA.

The package holds a decoder (word to operation and fields), an encoder (the
reverse, which the assembler `asm-rv64` uses), a step function that executes
one instruction against registers, memory, one serial device and the timer,
and a loader and a replay check for flat images.

The core owns no memory and reads no clock. The caller passes registers,
memory and the serial output buffer in as slices. Time is a count of steps,
so a run repeats exactly. No kernel has run on it yet. The section "Limits"
lists what is missing.

Exactness class: `exact-int`. Every result is an integer. The same starting
state and the same instruction give the same next state on every host, bit for
bit.

## Three profiles

A profile is the rule the step function follows when an instruction cannot
complete, and since 0.6.0 also the set of privilege modes the machine
has. The caller picks one when it makes the machine.

| Constructor | Profile | An instruction that cannot complete |
|---|---|---|
| `emu_new()` | report | The step changes nothing and returns a code that names the reason. This is the 0.1.0 contract, unchanged. Every instruction added since 0.1.0 is illegal in this profile: Zicsr, MRET, WFI, the M and A extensions, SRET and SFENCE.VMA. |
| `emu_new_machine()` | trap | The machine takes a trap: it records the cause in its CSRs and moves `pc` to the handler. The step returns `EMU_TRAP`. The timer, the interrupts and the M and A extensions exist in this profile and in the system profile. |
| `emu_new_system()` | system | Since 0.6.0. As the trap profile, with the modes S and U, trap delegation and Sv39 virtual memory. The sections from "System profile" on give its rules. |

The sections up to "Encodings" describe the report and trap profiles.
Where they say "the trap profile only", the system profile is included
unless "System profile" says otherwise.

## Machine

| Part | Model |
|---|---|
| Registers | `x`: caller slice of 32 words. `x[0]` reads 0 and ignores writes. |
| Memory | `ram`: caller slice of bytes at addresses `0x80000000` to `0x80000000 + ram.len() - 1`. Little-endian. A read or write may start at any byte address. |
| Serial device | Eight byte-wide registers at `0x10000000` to `0x10000007`, laid out like the common 16550 serial chip. A byte stored to offset 0 is appended to `out`. A load from offset 5, the line status register, gives `0x60`, which means "ready to send". A load from any other offset gives 0, and a store to any other offset has no effect. Only one-byte accesses reach the device. |
| `Machine.pc` | Address of the next instruction. Starts at `0x80000000`. |
| `Machine.steps` | Number of instructions retired. An instruction retires when it completes without a trap. |
| `Machine.traps` | Number of traps taken. Stays 0 in the report profile. |
| `Machine.out_len` | Number of bytes written to `out`. |
| Timer device | Trap profile only. `mtimecmp` is the 8 bytes at `0x02004000` and `mtime` the 8 bytes at `0x0200BFF8`. The section "Timer" gives the rules. |
| `Machine.mstatus`, `mtvec`, `mepc`, `mcause`, `mtval`, `mie`, `mip`, `mscratch` | The CSRs. `mstatus` starts at `0x1800`, the others at 0. |
| `Machine.mtime`, `Machine.mtimecmp` | The timer registers. `mtime` starts at 0 and `mtimecmp` at all ones, so no timer interrupt is pending until software sets `mtimecmp`. |
| `Machine.res`, `Machine.res_addr` | The reservation an LR leaves: its width in bytes, 4 or 8, or 0 for none, and its address. Both start at 0. The section "Reservation" gives the rules. |

Every other address is unmapped. In the report profile the timer addresses
are unmapped too.

## Step codes

`emu_step(m, x, ram, out)` executes the instruction at `m.pc` and returns a
code. It is total: it returns a code for every input.

| Code | Name | Report profile | Trap profile |
|---|---|---|---|
| 0 | `EMU_OK` | The instruction retired. `pc` moved and `steps` rose by 1. | The same. |
| 1 | `EMU_BREAK` | The instruction is EBREAK. | Not returned. EBREAK traps with cause 3. |
| 2 | `EMU_ECALL` | The instruction is ECALL. | Not returned. ECALL traps with cause 11. |
| 3 | `EMU_TRAP` | Not returned. | A trap or an interrupt was taken. `traps` rose by 1 and `steps` did not move. |
| -1 | `EMU_ILLEGAL` | The word is not an RV64I instruction. | Not returned. Cause 2. |
| -2 | `EMU_BUS` | The instruction, or the data it names, is outside mapped memory. | Not returned. Cause 1, 5 or 7. |
| -3 | `EMU_ALIGN` | `pc`, or the target of a taken jump or branch, is not a multiple of 4. | Not returned. Cause 0. |
| -4 | `EMU_SMALL` | A byte was sent to the serial device and `out` is full. | The same. The host empties `out` and steps again. |

In the report profile a step that returns any code other than `EMU_OK`
changes nothing. In the trap profile the same holds for `EMU_SMALL`, and a
step that returns `EMU_TRAP` changes only `pc`, `traps`, `mstatus`, `mepc`,
`mcause`, `mtval` and `mtime`: no register, no byte of memory, no byte of
output and not the reservation.

`emu_run(m, x, ram, out, max)` calls `emu_step` up to `max` times. In the
report profile it stops at the first code other than `EMU_OK` and returns it.
In the trap profile it continues through `EMU_TRAP`, stops only on
`EMU_SMALL`, and returns `EMU_OK` after `max` steps.

Rules the manual leaves to the environment, in both profiles: a load or store
at an address that is not a multiple of its size is allowed; FENCE completes
and does nothing; JALR clears bit 0 of its target and then the alignment rule
applies. The atomic instructions are the exception to the first rule: the
manual demands an aligned address for them, and the section "A extension"
gives what happens otherwise.

`emu_decode(w)` returns an `Insn` with operation number 0 when `w` is not an
instruction of this machine. `emu_encode(d)` returns the word for a decoded
instruction, or 0 when `d` names none. Operation numbers are in `src/abi.bld`.

## CSRs

WARL means "write any, read legal": software may write any value to the
field and the machine stores a legal one. The table gives the legal value
stored for a written value `v`.

| Number | Name | Reads | A write of `v` stores |
|---|---|---|---|
| `0x300` | `mstatus` | Stored value | `(v & 0x88) \| 0x1800`. Only MIE (bit 3, interrupts enabled) and MPIE (bit 7, the value MIE had before the last trap) are writable. MPP (bits 12 and 11, the privilege level before the last trap) always reads 3, machine mode, the only legal value here. Every other bit reads 0. |
| `0x301` | `misa` | `0x8000000000001101`: 2 in bits 63 and 62 for 64 bits, then one bit for each extension the machine has: bit 0 for A, bit 8 for the base set I, bit 12 for M. Before 0.5.0 the value was `0x8000000000000100`. | Nothing. The write is ignored and is legal, so software cannot switch an extension off. |
| `0x304` | `mie` | Stored value | `v & 0x888`: the machine software, timer and external enable bits (3, 7, 11). |
| `0x305` | `mtvec` | Stored value | `v & ~2`. Bits 1 and 0 are MODE, and only 0 (direct) and 1 (vectored) are legal, so bit 1 is cleared. The rest is BASE, a multiple of 4. |
| `0x340` | `mscratch` | Stored value | `v`, all 64 bits |
| `0x341` | `mepc` | Stored value | `v & ~3`. Instructions are 4 bytes, so the two low bits are always 0. |
| `0x342` | `mcause` | Stored value | `v`, all 64 bits |
| `0x343` | `mtval` | Stored value | `v`, all 64 bits |
| `0x344` | `mip` | Bits 3 and 11 of `Machine.mip`, and bit 7 computed from the timer | Nothing. The write is ignored and is legal. The section "Interrupts" gives the source of each bit. |
| `0xF11` to `0xF15` | `mvendorid`, `marchid`, `mimpid`, `mhartid`, `mconfigptr` | 0 | Read-only. An instruction that writes is illegal. |

Any other CSR number is illegal: the instruction traps with cause 2.

## Zicsr instructions

All six read the old value of the CSR into `rd` (nothing is written to `x0`)
and then may write the CSR. `uimm` is the 5-bit field where `rs1` would be,
used as an unsigned number.

| Instruction | New value | It counts as a write |
|---|---|---|
| CSRRW | `x[rs1]` | always |
| CSRRS | old \| `x[rs1]` | when the `rs1` field is not 0 |
| CSRRC | old & ~`x[rs1]` | when the `rs1` field is not 0 |
| CSRRWI | `uimm` | always |
| CSRRSI | old \| `uimm` | when `uimm` is not 0 |
| CSRRCI | old & ~`uimm` | when `uimm` is not 0 |

The test is on the field, not on the value: CSRRS with `rs1` = `x5` and
`x5` = 0 counts as a write. An instruction that does not count as a write
changes no CSR, and it is legal on a read-only CSR. An instruction that
counts as a write to `0xF11` to `0xF15` traps with cause 2 and changes no
register.

## Traps and MRET

Taking a trap does this, in one step:

1. `mepc` = `pc` of the trapping instruction, with the two low bits cleared.
2. `mcause` = the cause number, `mtval` = the value in the table.
3. In `mstatus`: MPIE = MIE, then MIE = 0. MPP stays 3.
4. `pc` = BASE of `mtvec`.

A synchronous trap is one caused by the instruction that is running. The
nine causes in the table below are synchronous, and they go to BASE in both
modes of `mtvec`. Interrupts follow the rules of the section "Interrupts".

| Cause | Name | When | `mtval` |
|---|---|---|---|
| 0 | Instruction address misaligned | A taken jump or branch names a target that is not a multiple of 4. `mepc` is the jump or branch, and no link register is written. Also when the host sets a `pc` that is not a multiple of 4. | The target address |
| 1 | Instruction access fault | Any of the 4 bytes at `pc` is outside memory. | `pc` |
| 2 | Illegal instruction | The word is not an instruction of this machine, names an unknown CSR, or writes a read-only CSR. | The word |
| 3 | Breakpoint | EBREAK | 0 |
| 4 | Load address misaligned | Since 0.5.0. An LR names an address that is not a multiple of its width. An ordinary load never takes this cause. | The address |
| 5 | Load access fault | A load or an LR reads a byte outside memory, other than a one-byte load from the serial device. | The first byte address of the access that is outside memory |
| 6 | Store/AMO address misaligned | Since 0.5.0. An SC or an AMO names an address that is not a multiple of its width. An ordinary store never takes this cause. | The address |
| 7 | Store/AMO access fault | A store, an SC or an AMO names a byte outside memory, other than a one-byte store to the serial device. Nothing is stored. | The same as for cause 5 |
| 11 | Environment call from machine mode | ECALL | 0 |

ECALL and EBREAK leave `mepc` on themselves, so a handler that wants to
continue adds 4 to `mepc`.

MRET sets MIE = MPIE, then MPIE = 1, leaves MPP at 3, and sets `pc` = `mepc`.
It retires, so `steps` rises by 1.

## Timer

The timer has two registers of 64 bits at the addresses the QEMU `virt`
machine uses for its core-local interruptor (CLINT), the block that holds
the timer on that machine. Only the two addresses were taken; no QEMU
program was downloaded or run.

| Register | Address | Start value | Meaning |
|---|---|---|---|
| `mtimecmp` | `0x02004000` to `0x02004007` | all ones | The value `mtime` is compared with. |
| `mtime` | `0x0200BFF8` to `0x0200BFFF` | 0 | The time: a count of steps. |

A load or store of 1, 2, 4 or 8 bytes may start at any byte of a register,
as long as every byte of the access is inside that register. Bytes are
little-endian: offset 0 is the least significant byte. A load reads the
bytes it names and extends them like a load from memory. A store replaces
the bytes it names and leaves the other bytes of the register.

An access with any byte outside the register takes the access fault of an
unmapped address: cause 5 for a load, cause 7 for a store. Nothing is read
or stored, and `mtval` holds the address of the first byte of the access,
even when that byte is inside the register.

`mtime` rises by 1 at the end of every step that returns `EMU_OK` or
`EMU_TRAP`. A step that returns `EMU_SMALL` changes nothing, `mtime`
included. The count wraps from all ones to 0. No host clock is read.

## Interrupts

A pending bit says that a source asks for an interrupt. An enable bit says
that software accepts it. `mip` holds the pending bits and `mie` the enable
bits, at the same positions.

| Cause | Name | Bit | Pending while |
|---|---|---|---|
| 11 | Machine external | MEIP, bit 11 | The host has set bit 11 of `Machine.mip`. |
| 3 | Machine software | MSIP, bit 3 | The host has set bit 3 of `Machine.mip`. |
| 7 | Machine timer | MTIP, bit 7 | `mtime >= mtimecmp`, compared as unsigned numbers. |

MTIP is computed each time it is needed. Software cannot write it, and the
host cannot set it: bit 7 of `Machine.mip` is ignored. Software clears the
timer interrupt by storing a later time to `mtimecmp`. No device inside the
machine sets MSIP or MEIP; only the host does.

Before the fetch, each step checks for an interrupt. One is taken when MIE
in `mstatus` is 1 and at least one bit is set in both `mip` and `mie`. When
more than one qualifies, the order is external, then software, then timer.

Taking interrupt `cause` does this in place of the instruction at `pc`:

1. `mepc` = `pc`, the instruction that did not execute, with the two low
   bits cleared.
2. `mcause` = 2^63 + `cause`. Bit 63 marks an interrupt. `mtval` = 0.
3. In `mstatus`: MPIE = MIE, then MIE = 0. MPP stays 3.
4. `pc` = BASE of `mtvec` in direct mode, or BASE + 4 x `cause` in vectored
   mode: `0x0C` past BASE for software, `0x1C` for timer, `0x2C` for external.

The step returns `EMU_TRAP`. `traps` rises by 1 and `steps` does not move.
No register, no byte of memory and no byte of output changes.

Entry clears MIE, so a handler is not interrupted unless it sets MIE again.
MRET restores MIE, and an interrupt that is still pending and enabled is
taken by the very next step, before the instruction at `mepc` runs.

## Order within a step

The order decides which step sees a timer interrupt, so it is fixed and
checked. One step in the trap profile does three things in this order:

1. The interrupt check. It reads the `mtime` that the previous step left.
2. Either the interrupt is taken, or one instruction is fetched and
   executed. A load from `mtime` reads the same value the check read.
3. The tick: `mtime` rises by 1.

Two consequences. A store of the value `V` to all of `mtime` leaves `V` + 1
after the step. With `mtimecmp` = `T`, MIE = 1 and the timer enabled, step
number `T` + 1 of a run that started at `mtime` = 0 is the first that takes
the interrupt, because it is the first that starts with `mtime` = `T`.
Control 19 is a copy of the core that ticks before the check, and the checks
refuse it.

## WFI

WFI is the word `0x10500073`. In the trap profile it retires as a no-op:
`pc` moves by 4, `steps` rises by 1, and the machine does not wait. The
manual permits this. A caller that wants to skip idle time can see the WFI
in memory and advance `mtime` itself before the next step. When an
interrupt is pending and enabled at the start of the step, it is taken in
place of the WFI and `mepc` holds the address of the WFI. In the report
profile WFI is illegal, like every instruction added since 0.1.0.

## M extension

Since 0.5.0 the trap profile executes the M extension. An M instruction
reads `rs1` and `rs2`, writes `rd` and never traps.

| Instruction | Value written to `rd` |
|---|---|
| MUL | Bits 63 to 0 of the product `rs1` x `rs2`. |
| MULH | Bits 127 to 64 of the product, with both operands read as signed numbers. |
| MULHSU | Bits 127 to 64, with `rs1` signed and `rs2` unsigned. |
| MULHU | Bits 127 to 64, with both unsigned. |
| DIV, DIVU | The quotient of `rs1` by `rs2`, signed or unsigned, rounded toward zero. |
| REM, REMU | The remainder of that division. A nonzero result of REM has the sign of the dividend. |
| MULW | Bits 31 to 0 of the product of the low 32 bits of each operand, sign-extended to 64 bits. |
| DIVW, DIVUW | The 32-bit quotient of the low 32 bits of `rs1` by the low 32 bits of `rs2`, signed or unsigned, sign-extended to 64 bits. |
| REMW, REMUW | The 32-bit remainder of that division, sign-extended to 64 bits. |

Two cases have fixed results and no trap. L is 64, and 32 for the five
forms whose name ends in W.

| Case | Dividend | Divisor | DIVU, DIVUW | REMU, REMUW | DIV, DIVW | REM, REMW |
|---|---|---|---|---|---|---|
| Division by zero | x | 0 | 2^L - 1 | x | -1 | x |
| Signed overflow | -2^(L-1) | -1 | cannot occur | cannot occur | -2^(L-1) | 0 |

A 32-bit result is sign-extended in these cases too. DIVUW by zero writes
64 ones, and REMUW of `0x80000000` by zero writes `0xFFFFFFFF80000000`.
Outside the two cases, quotient x divisor + remainder is the dividend.

BuildLang has integers of 64 bits and none of 128. The core forms the upper
half of an unsigned product from four products of 32-bit halves, and
corrects it for the signed forms: an operand that is negative was read
2^64 too large, so the other operand is subtracted once. It divides the
magnitudes with the unsigned division of the host and sets the signs
after. `src/lib6.bld` quotes the sentence of the manual beside each rule.

## A extension

Since 0.5.0 the trap profile executes the A extension. A hart is one
hardware thread of execution, and this machine has one. An atomic
instruction is one whose read and write of memory no other hart can come
between. With one hart nothing can come between in any case, so here the
instructions are ordinary reads and writes with the rules below.

The address is the value of `rs1`. There is no offset. The width is 4 bytes
for the forms that end in `.W` and 8 bytes for those that end in `.D`.

| Instruction | Effect |
|---|---|
| LR.W, LR.D | Reads the value at the address into `rd` and takes the reservation. |
| SC.W, SC.D | Stores `rs2` at the address when the reservation allows it, and writes 0 to `rd`. Otherwise it stores nothing and writes 1 to `rd`. Either way the reservation is gone after it. |
| AMOSWAP | Reads the old value into `rd` and stores `rs2`. |
| AMOADD, AMOXOR, AMOAND, AMOOR | Reads the old value into `rd` and stores old + `rs2`, old xor `rs2`, old and `rs2`, old or `rs2`. |
| AMOMIN, AMOMAX | Reads the old value into `rd` and stores the smaller or the larger of old and `rs2`, compared as signed numbers. |
| AMOMINU, AMOMAXU | The same, compared as unsigned numbers. |

Each AMO exists in both widths. In the 32-bit forms the value read is
sign-extended into `rd`, the upper 32 bits of `rs2` are ignored, the
comparison is on 32 bits and 32 bits are stored. The source is read before
`rd` is written, so `rd` may be the same register as `rs2` or `rs1`.

Two bits of the word, `aq` and `rl`, ask for an order of memory accesses
as other harts see them. The machine accepts every value of them and
ignores them. The decoder keeps them in the `imm` field of the decoded
record, bit 1 for `aq` and bit 0 for `rl`, so that the encoder gives the
word back.

Before any effect, two checks run in this order.

1. Alignment. The address must be a multiple of the width. If not, the
   instruction traps: cause 4 for LR and cause 6 for SC and the AMOs, with
   the address in `mtval`.
2. Access. Every byte must be in memory, or in one timer register. If not,
   LR traps with cause 5 and SC and the AMOs with cause 7. `mtval` is the
   first byte address of the access that is outside memory, as for a load
   or store. An SC takes this trap also when no reservation is held: the
   check comes before the question whether it stores.

A trap changes no register, no byte of memory and not the reservation.

Devices behave as for loads and stores of the same width. An atomic
instruction may name `mtimecmp` or `mtime` at offset 0, or at offset 4 in
the 32-bit forms: it reads the bytes, and SC and the AMOs replace them.
The tick follows the store, so an SC that stores 77 to `mtime` leaves 78.
Only one-byte accesses reach the serial device, so an atomic instruction
there takes the access fault.

## Reservation

The reservation is the mark LR leaves and SC checks. It is a width and an
address, in `Machine.res` and `Machine.res_addr`.

| Event | The reservation after it |
|---|---|
| Reset, and the start of a run (`rr_boot`) | None. |
| An LR retires | The width and the address of that LR. An older reservation is replaced. |
| An SC retires, whether it stored or not | None. |
| An AMO, a load, a store or any other instruction retires | Unchanged, also when the instruction writes the reserved bytes. |
| A trap or an interrupt is taken, or MRET retires | Unchanged. |

An SC stores when a reservation is held and both its width and its address
are those of the SC. So after an LR, the next SC to the same address and
of the same width always stores, and a second SC never does.

Which of this the manual fixes and which is a choice of this machine:

| Rule here | Manual |
|---|---|
| An SC ends the reservation. | "Regardless of success or failure, executing an SC.W instruction invalidates any reservation held by this hart." |
| A successful SC writes 0 to `rd` and a failed one 1. | "If the SC.W succeeds, the instruction writes the word in rs2 to memory, and it writes zero to rd. If the SC.W fails, the instruction does not write to memory, and it writes a nonzero value to rd." "The failure code with value 1 encodes an unspecified failure." |
| The reservation covers exactly the bytes the LR read. | A choice. LR "registers a reservation set: a set of bytes that subsumes the bytes in the addressed word", and "An implementation can register an arbitrarily large reservation set on each LR". The smallest set is taken. |
| An SC to another address fails. | "The SC must fail if the address is not within the reservation set of the most recent LR in program order." |
| An SC of the other width at the same address fails. | A choice, stricter than the text asks. A loop the manual guarantees to succeed has an SC "to the same effective address and of the same data size as the latest LR", and "Implementations are permitted to unconditionally fail any unconstrained LR/SC sequence." |
| A store or an AMO of this hart leaves the reservation. | A choice. The text demands failure for a store "from another hart" and for a write "from some other device". There is no other hart, and no device here writes memory. |
| A trap, an interrupt and MRET leave the reservation. | A choice the text permits: "the xRET instruction is allowed to clear any outstanding LR address reservation but is not required to. Trap handlers should explicitly clear the reservation if required (e.g., by using a dummy SC) before executing the xRET." |
| No reservation after reset. | "For implementations with the Zalrsc standard extension, there is no valid load reservation." |
| A misaligned address traps with the address-misaligned cause, before the access check. | "If the address is not naturally aligned, an address-misaligned exception or an access-fault exception will be generated." The first of the two is the choice. For the order: "Load/store/AMO address-misaligned exceptions may have either higher or lower priority than load/store/AMO page-fault and access-fault exceptions." |
| An SC with no reservation still takes the access fault. | "No SC.W instruction shall retire unless it passes memory permission checks". |

## Encodings

Every row is a row of the instruction listing in `src/unpriv/rv-32-64g.adoc`
of the manual. `aq` is word bit 26 and `rl` bit 25.

| Instructions | Bits 31 to 25 | Bits 24 to 20 | Bits 14 to 12 | Bits 6 to 0 |
|---|---|---|---|---|
| MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU | `0000001` | `rs2` | `000` to `111` in this order | `0110011` |
| MULW, DIVW, DIVUW, REMW, REMUW | `0000001` | `rs2` | `000`, `100`, `101`, `110`, `111` | `0111011` |
| LR | `00010`, `aq`, `rl` | `00000` | `010` for .W, `011` for .D | `0101111` |
| SC | `00011`, `aq`, `rl` | `rs2` | the same | `0101111` |
| AMOSWAP, AMOADD, AMOXOR, AMOAND, AMOOR | `00001`, `00000`, `00100`, `01100`, `01000`, each with `aq`, `rl` | `rs2` | the same | `0101111` |
| AMOMIN, AMOMAX, AMOMINU, AMOMAXU | `10000`, `10100`, `11000`, `11100`, each with `aq`, `rl` | `rs2` | the same | `0101111` |

An LR word with a value other than `00000` in bits 24 to 20 is illegal
here. A word with the opcode `0101111` and any other value in bits 14 to 12
or bits 31 to 27 is illegal. Operation numbers 61 to 95 are in
`src/abi.bld`.

## System profile

Since 0.6.0 the package has a third profile, made by `emu_new_system()`.
It is the trap profile with two more privilege modes and with virtual
memory. Everything the sections above say about the trap profile holds in
it, except where this section and the ones after it say otherwise. The
trap profile itself is unchanged: a machine made by `emu_new_machine()`
has machine mode only, as in 0.5.0, and every word and CSR number this
version adds is illegal there.

A privilege mode is the level of trust the processor is in. It decides
which instructions and which CSRs a program may use. The machine has
three, and `Machine.prv` holds the present one.

| Mode | Number | Meant for |
|---|---|---|
| M, machine | 3 | The first code after reset. It may do everything. |
| S, supervisor | 1 | An operating system kernel. |
| U, user | 0 | A program the kernel runs. |

"Upon reset, a hart's privilege mode is set to M." `misa` reads
`0x8000000000141101` in this profile: the value of the trap profile with
bit 18 for S and bit 20 for U.

The state the profile adds to the machine:

| Field | Meaning | Start value |
|---|---|---|
| `Machine.prv` | The privilege mode. | 3 |
| `Machine.medeleg`, `Machine.mideleg` | The delegation registers. | 0 |
| `Machine.stvec`, `sscratch`, `sepc`, `scause`, `stval` | The supervisor trap registers. | 0 |
| `Machine.satp` | The register that turns translation on and names the root page table. | 0 |
| `Machine.menvcfg`, `Machine.senvcfg` | The environment configuration registers. | 0 |

`mstatus` starts at `0xA00001800`: MPP = 3 as in the trap profile, and the
two read-only fields UXL and SXL, both 2, which say that S-mode and U-mode
run with 64-bit registers.

The step codes, the timer, the order within a step, the M and A
extensions, the serial device and the flat image are those of the trap
profile. `emu_run` treats the profile like the trap profile.

## CSRs of the system profile

The profile has the CSRs of the trap profile, with more writable bits in
some of them, and fifteen more. The table gives the value stored for a
written value `v`. The bit positions of `mstatus` come from the field list
of the manual's figure for the register.

| Number | Name | Reads | A write of `v` stores |
|---|---|---|---|
| `0x300` | `mstatus` | Stored value | The bits of `v` in the mask `0x7E19AA`: SIE (1), MIE (3), SPIE (5), MPIE (7), SPP (8), MPP (12 and 11), MPRV (17), SUM (18), MXR (19), TVM (20), TW (21), TSR (22). A write of 2 to MPP leaves MPP as it was. UXL (33 and 32) and SXL (35 and 34) always read 2. Every other bit reads 0. |
| `0x100` | `sstatus` | `mstatus` and `0x2000C0122`: SIE, SPIE, SPP, SUM, MXR and UXL | The bits of `v` in the mask `0xC0122` go to the same bits of `mstatus`. No other bit of `mstatus` changes. |
| `0x301` | `misa` | `0x8000000000141101` | Nothing. |
| `0x302` | `medeleg` | Stored value | `v & 0xB3FF`: one bit for each of the causes 0 to 9, 12, 13 and 15. |
| `0x303` | `mideleg` | Stored value | `v & 0x222`: the three supervisor interrupts. |
| `0x304` | `mie` | Stored value | `v & 0xAAA`: the enable bits of all six interrupts. |
| `0x104` | `sie` | `mie & mideleg` | The bits of `v` that `mideleg` has set go to `mie`. |
| `0x344` | `mip` | The pending bits. The section "Interrupts in the system profile" gives the source of each. | Bits 1, 5 and 9 of `v`: SSIP, STIP and SEIP. Bits 3, 7 and 11 are read-only. |
| `0x144` | `sip` | `mip & mideleg` | Bit 1 of `v`, SSIP, when `mideleg` has bit 1 set. Nothing else. |
| `0x305`, `0x105` | `mtvec`, `stvec` | Stored value | `v & ~2`, as `mtvec` in the trap profile. |
| `0x341`, `0x141` | `mepc`, `sepc` | Stored value | `v & ~3` |
| `0x340`, `0x342`, `0x343`, `0x140`, `0x142`, `0x143` | `mscratch`, `mcause`, `mtval`, `sscratch`, `scause`, `stval` | Stored value | `v`, all 64 bits |
| `0x180` | `satp` | Stored value | See below. |
| `0x306`, `0x106` | `mcounteren`, `scounteren` | 0 | Nothing. |
| `0x30A`, `0x10A` | `menvcfg`, `senvcfg` | Stored value | `v & 1`: the bit FIOM, which has no effect here. |
| `0xF11` to `0xF15` | as in the trap profile | 0 | Read-only. |

`satp` has three fields: MODE in bits 63 to 60, ASID in bits 59 to 44 and
PPN, the page number of the root page table, in bits 43 to 0. A write
depends on the MODE it names:

| MODE in `v` | Effect |
|---|---|
| 8, Sv39 | `satp` = MODE 8, ASID 0, and the PPN of `v`. |
| 0, Bare | `satp` = 0, whatever the other fields of `v` hold. |
| any other | None. `satp` keeps its value. |

Two rules decide whether a CSR instruction may touch a CSR at all. Bits 9
and 8 of the number are the lowest mode that may access it: the numbers
`0x1xx` need S-mode or M-mode, `0x3xx` and `0xFxx` need M-mode. Bits 11
and 10 both set mark a read-only register. An instruction that breaks
either rule, or names a number that is not in the table, traps with cause
2 and changes nothing. A third rule belongs to one register: in S-mode
with TVM = 1 every access to `satp` traps with cause 2.

Which of this the manual fixes and which is a choice of this machine:

| Rule here | Manual |
|---|---|
| The access rules from the number. | "The top two bits (csr[11:10]) indicate whether the register is read/write (00, 01, or 10) or read-only (11). The next two bits (csr[9:8]) encode the lowest privilege level that can access the CSR". "Attempts to access a CSR address without sufficient privilege raise illegal-instruction exceptions". "Attempts to write a read-only CSR address raise illegal-instruction exceptions." |
| A CSR that does not exist traps with cause 2. | A choice, as since 0.2.0: "Instructions that access a non-existent CSR address are reserved." |
| `sstatus` is a view. | "The sstatus register is a subset of the mstatus register." |
| MPP holds 0, 1 or 3, and a write of 2 leaves it. | "xPP fields are WARL fields that can hold only privilege mode x and any implemented privilege mode lower than x." Which legal value a write of 2 gives is a choice. |
| SXL and UXL read 2 and cannot be written. | "an implementation may make SXL be a read-only field whose value always ensures that SXLEN=MXLEN", and the same sentence for UXL. |
| The fields of `mstatus` this machine does not have read 0: UBE, SBE, MBE (no big-endian access), VS, FS, XS and SD (no vector or floating-point state), SPELP, MPELP, SDT, MDT, GVA and MPV (extensions that are absent). | Each is WARL or belongs to an extension, and the text lets it be read-only 0 when the extension is absent. A choice for each. |
| `sie` and `sip` show only delegated interrupts. | "If an interrupt is delegated to S-mode by setting a bit in the mideleg register, it becomes visible in the sip register and is maskable using the sie register. Otherwise, the corresponding bits in sip and sie are read-only zero." |
| SSIP, STIP and SEIP can be written in `mip`. | "SEIP is writable in mip", "If the stimecmp register is not implemented, STIP is writable in mip", "SSIP is writable in mip". |
| Only SSIP can be written in `sip`. | "If implemented, SEIP is read-only in sip", "If implemented, STIP is read-only in sip", "If implemented, SSIP is writable in sip". |
| `medeleg` has bits for the causes this machine can raise below M-mode. | "medeleg[11] is read-only zero." "An implementation can choose to subset the delegatable traps". |
| The machine interrupts cannot be delegated. | A choice the text permits: an implementation "shall not fix as read-only one any bits of mideleg corresponding to machine-level interrupts", and here they are read-only zero. |
| `satp` ignores a write of a MODE it does not support. | "Implementations are not required to support all MODE settings, and if satp is written with an unsupported MODE, the entire write has no effect; no fields in satp are modified." |
| No ASID bit. | "The number of ASID bits is UNSPECIFIED and may be zero." |
| A write of MODE 0 stores 0. | A choice. "To select MODE=Bare, software must write zero to the remaining fields of satp", and a write that does not "has an UNSPECIFIED effect on the value that the remaining fields assume". |
| `mcounteren` and `scounteren` exist and read 0. | "In harts with U-mode, the mcounteren must be implemented, but all fields are WARL and may be read-only zero". "scounteren must be implemented. However, any of the bits may be read-only zero". |
| `menvcfg` and `senvcfg` exist with FIOM alone. | "In harts with U-mode, menvcfg must be implemented, but all WARL fields may be read-only zero." FIOM is kept writable because the text allows it to be read-only zero only "If S-mode is not supported, or if satp.MODE is read-only zero". It changes the order of device accesses as other harts see them, and with one hart it has no effect. |

## Traps in the system profile

Delegation is the rule that lets a trap enter the supervisor handler
directly, without passing through machine mode. `medeleg` has one bit for
each synchronous cause and `mideleg` one for each interrupt.

A trap goes to S-mode when its bit is set and the machine is in S-mode or
U-mode. In every other case it goes to M-mode. A trap taken in M-mode
always stays in M-mode. The entry writes the registers of the mode that
takes the trap and leaves those of the other mode alone.

| Entry into | Registers written | Status stack | New `pc` |
|---|---|---|---|
| M-mode | `mepc` = `pc` with the two low bits cleared, `mcause`, `mtval` | MPIE = MIE, MIE = 0, MPP = the mode before | BASE of `mtvec` |
| S-mode | `sepc`, `scause`, `stval`, the same way | SPIE = SIE, SIE = 0, SPP = 0 when the mode before was U, else 1 | BASE of `stvec` |

The mode becomes the mode of the handler. An interrupt goes to BASE + 4 x
cause when the vector register of the handler's mode is in vectored mode.
The reservation is not changed by a trap.

The synchronous causes are those of the trap profile and five more:

| Cause | Name | When | Trap value |
|---|---|---|---|
| 8 | Environment call from U-mode | ECALL in U-mode | 0 |
| 9 | Environment call from S-mode | ECALL in S-mode | 0 |
| 12 | Instruction page fault | Translation refuses the fetch. | `pc` |
| 13 | Load page fault | Translation refuses a load or an LR. | The virtual address, see "Memory access in the system profile" |
| 15 | Store/AMO page fault | Translation refuses a store, an SC or an AMO. | The same |

ECALL in M-mode still has cause 11. The access faults 1, 5 and 7 keep
their meaning, with the virtual address as trap value, and one more case:
a page table entry that the walk needs lies outside memory.

MRET and SRET return from a handler. With x for M or S:

1. The mode becomes the value of xPP.
2. xIE = xPIE, then xPIE = 1.
3. xPP = 0, the user mode.
4. MPRV = 0 when the new mode is not M.
5. `pc` = `mepc` or `sepc`.

MRET in S-mode or U-mode traps with cause 2. SRET in U-mode traps with
cause 2, and so does SRET in S-mode while TSR = 1. In M-mode SRET is
legal whatever TSR holds. Both retire, so `steps` rises by 1. Neither
changes the reservation.

Two more instructions depend on a bit of `mstatus`:

| Instruction | Word | Traps with cause 2 | Otherwise |
|---|---|---|---|
| WFI | `0x10500073` | In S-mode or U-mode while TW = 1. | Retires as a no-op, in every mode. |
| SFENCE.VMA `rs1`, `rs2` | `0x12000073` with `rs1` in bits 19 to 15 and `rs2` in bits 24 to 20 | In U-mode, and in S-mode while TVM = 1. | Retires as a no-op. |
| SRET | `0x10200073` | As above. | As above. |

The rows of the manual's listing of privileged instructions give SRET as
`0001000 00010 00000 000 00000 1110011`, MRET as `0011000 00010 ...`, WFI
as `0001000 00101 ...` and SFENCE.VMA as `0001001 rs2 rs1 000 00000
1110011`. The operation numbers are 96 for SRET and 97 for SFENCE.VMA.
The decoder and the encoder know both in every profile; the report
profile and the trap profile refuse to execute them.

Which of this the manual fixes:

| Rule here | Manual |
|---|---|
| Delegation, and never from M-mode. | "setting a bit in medeleg or mideleg will delegate the corresponding trap, when occurring in S-mode or U-mode, to the S-mode trap handler." "Traps never transition from a more-privileged mode to a less-privileged mode." |
| What the entry into S-mode writes. | "When a trap is delegated to S-mode, the scause register is written with the trap cause; the sepc register is written with the virtual address of the instruction that took the trap; the stval register is written with an exception-specific datum; the SPP field of mstatus is written with the active privilege mode at the time of the trap; the SPIE field of mstatus is written with the value of the SIE field at the time of the trap; and the SIE field of mstatus is cleared. The mcause, mepc, and mtval registers and the MPP and MPIE fields of mstatus are not written." |
| The status stack at entry. | "When a trap is taken from privilege mode y into privilege mode x, xPIE is set to the value of xIE; xIE is set to 0; and xPP is set to y." |
| The two returns. | "When executing an xRET instruction, supposing xPP holds the value y, xIE is set to xPIE; the privilege mode is changed to y; xPIE is set to 1; and xPP is set to the least-privileged supported mode (U if U-mode is implemented, else M). If y != M, xRET also sets MPRV=0." |
| Who may return. | "An xRET instruction can be executed in privilege mode x or higher". "Attempting to execute an xRET instruction in a mode less privileged than x will raise an illegal-instruction exception." |
| TSR. | "When TSR=1, attempts to execute SRET while executing in S-mode will raise an illegal-instruction exception." |
| TVM. | "When TVM=1, attempts to read or write the satp CSR or execute an SFENCE.VMA or SINVAL.VMA instruction while executing in S-mode will raise an illegal-instruction exception." |
| TW. | A choice the text permits: "An implementation may have WFI always raise an illegal-instruction exception in modes less privileged than M when TW=1". |
| WFI in U-mode with TW = 0 retires. | A choice. "When S-mode is implemented, then executing WFI in U-mode causes an illegal-instruction exception, regardless of the value of the TW bit, unless the instruction completes within an implementation-specific, bounded time limit." Here WFI never waits, so it completes at once. |
| The three causes of ECALL. | "When executed in U-mode, S-mode, or M-mode, it generates an environment-call-from-U-mode exception, environment-call-from-S-mode exception, or environment-call-from-M-mode exception, respectively". |
| A trap and a return leave the reservation. | A choice the text permits, as in 0.5.0: "the xRET instruction is allowed to clear any outstanding LR address reservation but is not required to." The text names no effect of a trap entry on the reservation. |

## Interrupts in the system profile

There are six interrupts. The three of the trap profile are joined by
three for the supervisor.

| Cause | Name | Bit of `mip` | Pending while |
|---|---|---|---|
| 11 | Machine external, MEI | 11 | The host has set bit 11 of `Machine.mip`. |
| 3 | Machine software, MSI | 3 | The host has set bit 3. |
| 7 | Machine timer, MTI | 7 | `mtime >= mtimecmp` |
| 9 | Supervisor external, SEI | 9 | Bit 9 of `Machine.mip` is set. Machine-mode software writes it through `mip`. |
| 1 | Supervisor software, SSI | 1 | Bit 1 is set. Software writes it through `mip`, or through `sip` when it is delegated. |
| 5 | Supervisor timer, STI | 5 | Bit 5 is set. Machine-mode software writes it through `mip`. |

There is no supervisor timer register (the Sstc extension is absent), so
the supervisor timer interrupt is pending exactly while machine-mode
software holds STIP set. There is no interrupt controller: no device sets
SEIP, and the host may set any of the five stored bits in `Machine.mip`.

Before the fetch, each step checks for an interrupt. Interrupt i can be
taken when bit i is set in both `mip` and `mie` and the present mode
allows its destination:

| Destination | Condition on i | Allowed when |
|---|---|---|
| M-mode | Bit i of `mideleg` is clear. | The mode is S or U, or the mode is M and MIE = 1. |
| S-mode | Bit i of `mideleg` is set. | The mode is U, or the mode is S and SIE = 1. Never in M-mode. |

So a delegated interrupt is taken in U-mode whatever SIE holds, and an
interrupt for M-mode is taken in S-mode and U-mode whatever MIE holds.
When several can be taken, every interrupt for M-mode comes before every
interrupt for S-mode, and inside each group the order is MEI, MSI, MTI,
SEI, SSI, STI.

| Rule here | Manual |
|---|---|
| To M-mode. | "An interrupt i will trap to M-mode (causing the privilege mode to change to M-mode) if all of the following are true: (a) either the current privilege mode is M and the MIE bit in the mstatus register is set, or the current privilege mode has less privilege than M-mode; (b) bit i is set in both mip and mie; and (c) if register mideleg exists, bit i is not set in mideleg." |
| To S-mode. | "An interrupt i will trap to S-mode if both of the following are true: (a) either the current privilege mode is S and the SIE bit in the sstatus register is set, or the current privilege mode has less privilege than S-mode; and (b) bit i is set in both sip and sie." |
| The order. | "Interrupts to M-mode take priority over any interrupts to lower privilege modes." "Multiple simultaneous interrupts destined for M-mode are handled in the following decreasing priority order: MEI, MSI, MTI, SEI, SSI, STI, LCOFI." "Multiple simultaneous interrupts destined for supervisor mode are handled in the following decreasing priority order: SEI, SSI, STI, LCOFI." |
| STIP comes from machine-mode software. | "When the Sstc extension is not implemented, STIP is set and cleared by the execution environment." |

## Sv39 translation

A virtual address is the address a program uses. A physical address is
the address of a byte of the machine: memory, the serial device or the
timer. Translation turns a virtual address into a physical one. A page is 4096 bytes
that are translated together. A page table is one page that holds 512
page table entries (PTEs) of 8 bytes; an entry either points to the next
page table or, as a leaf, names the page an address belongs to. Sv39 is
the scheme with three levels of page tables and 39 bits of virtual
address.

Translation is on for an access when `satp` holds MODE 8 and the mode
the access is checked in is S or U. That mode is the present mode for a
fetch. For a load or a store it is MPP when MPRV = 1 and the present
mode otherwise. In every other case, M-mode and `satp` MODE 0, the
physical address is the virtual address and nothing is checked.

An access has a kind: a fetch, a load (also LR) or a store (also SC and
the AMOs). A page fault of a kind has cause 12, 13 or 15.

1. A canonical address is one whose bits 63 to 39 all equal bit 38. An
   address that is not canonical takes the page fault.
2. The address has three 9-bit fields, VPN[2] in bits 38 to 30, VPN[1]
   in bits 29 to 21 and VPN[0] in bits 20 to 12, and a 12-bit page
   offset. The walk starts at the root table, at PPN of `satp` x 4096,
   with level 2.
3. The PTE of the level is the doubleword at table + VPN[level] x 8. If
   any of its 8 bytes lies outside memory, the access fault of the kind
   is taken (cause 1, 5 or 7).
4. The PTE has the flags V (bit 0), R, W, X, U, G, A and D (bit 7), two
   bits for software, the page number PPN in bits 53 to 10, and reserved
   bits 63 to 54. The page fault is taken when V = 0, when W = 1 and
   R = 0, or when any of bits 63 to 54 is set.
5. With R = 0 and X = 0 the PTE points to the next table, at its PPN x
   4096. The page fault is taken when D, A or U is set in such a PTE, and
   when the level is already 0. Otherwise the walk goes on at step 3 one
   level down.
6. Otherwise the PTE is a leaf. A leaf at level 1 or 2 is a superpage:
   it maps 2 MiB or 1 GiB at once. The low 9 bits of its PPN at level 1,
   and the low 18 at level 2, must be 0, or the page fault is taken.
7. The U bit. In U-mode the page needs U = 1. In S-mode a page with
   U = 1 is refused for a fetch always, and for a load or a store unless
   SUM = 1. A page with U = 0 is refused in U-mode.
8. The permission bits. A fetch needs X. A load needs R, or X when
   MXR = 1. A store needs W.
9. The page fault is taken when A = 0, and for a store when D = 0. The
   machine never writes a PTE.
10. The physical address takes the page offset of the virtual address,
    the VPN fields below the level of the leaf, and the PPN of the leaf
    above them.

G and the two software bits are ignored. Steps 7 to 9 are the same for a
superpage.

| Rule here | Manual |
|---|---|
| Canonical addresses. | Addresses "must have bits 63-39 all equal to bit 38, or else a page-fault exception will occur". |
| The walk. | The ten steps of "Virtual Address Translation Process" with "LEVELS equals 3 and PTESIZE equals 8". Step 3 of the text: "If pte.v=0, or if pte.r=0 and pte.w=1, or if any bits or encodings that are reserved for future standard use are set within pte, stop and raise a page-fault exception". |
| A PTE outside memory. | "If accessing pte violates a PMA or PMP check, raise an access-fault exception corresponding to the original access type." |
| Bits 63 to 54. | Bit 63: "If Svnapot is not implemented, bit 63 remains reserved and must be zeroed by software for forward compatibility, or else a page-fault exception is raised". Bits 62 and 61: the same sentence for Svpbmt. Bits 60 to 54: "If any of these bits are set, a page-fault exception is raised". |
| D, A and U in a pointer. | "For non-leaf PTEs, the D, A, and U bits are reserved for future standard use." They fall under step 3 of the text. This reading is a choice: the text does not say in so many words that such a PTE faults. |
| Superpages. | "If i>0 and pte.ppn[i-1:0] != 0, this is a misaligned superpage; stop and raise a page-fault exception". |
| U and SUM. | "U-mode software may only access the page when U=1. If the SUM bit in the sstatus register is set, supervisor mode software may also access pages with U=1." "Irrespective of SUM, the supervisor may not execute code on pages with U=1." |
| MXR. | "When MXR=1, loads from pages marked either readable or executable (R=1 or X=1) will succeed." |
| MPRV. | "When MPRV=1, load and store memory addresses are translated and protected, and endianness is applied, as though the current privilege mode were set to MPP. Instruction address-translation and protection are unaffected by the setting of MPRV." |
| A and D. | The Svade scheme: "when a virtual page is accessed and the A bit is clear, or is written and the D bit is clear, a page-fault exception is raised." The text defines two schemes and adds, of the other one: "Simple implementations may instead generate page-fault exceptions." With Svadu absent, "the Svade extension, which mandates exceptions when A/D bits need be set, instead takes effect." |
| The fault kinds. | "Attempting to fetch an instruction from a page that does not have execute permissions raises a fetch page-fault exception." The same for a load or load-reserved and a load page fault, and for "a store, store-conditional, AMO" and a store page fault. "AMOs never raise load page-fault exceptions." |
| MODE 0. | "When MODE=Bare, supervisor virtual addresses are equal to supervisor physical addresses, and there is no additional memory protection beyond the physical memory protection scheme". |

MPRV is honoured in every mode. Architecturally it can be 1 only in
M-mode, because both returns clear it and only M-mode can write it. The
host can set it in any mode, and the checks do.

### No translation cache

A real processor keeps translations it has worked out in a cache, and
SFENCE.VMA is the instruction that tells it to forget them after software
has changed a page table. This machine keeps none. Every fetch, load and
store walks the page tables as they are in memory at that moment, so
SFENCE.VMA does its privilege check and nothing else.

What this means for a program: a change to a page table or to `satp`
takes effect at the very next access. A kernel that forgets SFENCE.VMA
runs correctly here and may fail on a machine with a cache, where the old
translation can stay in use. This machine cannot show that fault.

### No physical memory protection

Physical memory protection (PMP) is a set of machine-mode registers that
limit which physical addresses S-mode and U-mode may use. The machine has
none of them, and their CSR numbers do not exist. The text gives the
result: "If no PMP entry matches an S-mode or U-mode memory operation,
the operation fails if at least one PMP entry is implemented and succeeds
otherwise." No entry is implemented, so the operation succeeds: with
`satp` MODE 0, S-mode and U-mode reach every physical address M-mode
reaches.

## Memory access in the system profile

A load or a store of n bytes at a virtual address may cross from one page
into the next, because the machine allows an address that is not a
multiple of the size. Each page is translated by itself.

1. The page of the first byte is translated. A fault is taken with the
   address of the access as trap value.
2. If the access crosses into the next page, that page is translated. A
   fault is taken with the address of the first byte in that page as trap
   value.
3. The physical check. An access inside one page may name the serial
   device (one byte), one timer register, or memory, as in the trap
   profile. An access across two pages must lie in memory with both
   parts. Otherwise the access fault of the kind is taken (cause 5 or
   7), and the trap value is the virtual address of the first byte that
   is outside memory.
4. The access happens. The two parts of a crossing access may lie
   anywhere in memory.

All three checks come before any effect, so a trap changes no register,
no byte of memory, no byte of output and not the reservation. This is
stricter than the text asks, which allows that "a portion of a misaligned
store that passes the exception check may become visible, even if another
portion fails the exception check."

The atomic instructions check alignment first, on the virtual address,
with cause 4 or 6 as in the trap profile. Then the address is translated,
as a load for LR and as a store for SC and the AMOs, then the physical
check runs, then the effect of the section "A extension". An SC is
translated as a store also when it holds no reservation. The reservation
holds the physical address, so an SC through another virtual address of
the same page succeeds. An aligned access never crosses a page, which
keeps the reservation inside one page: "the LR/SC reservation set must
lie completely within a single base physical page".

The fetch translates `pc`. A page fault or an access fault of the fetch
has `pc` as trap value. An instruction is 4 bytes at a multiple of 4, so
a fetch never crosses a page.

## State hash, images and commands in the system profile

In the system profile the state hash takes eleven more words after the
fields of the machine and the reservation: `prv`, `medeleg`, `mideleg`,
`stvec`, `sscratch`, `sepc`, `scause`, `stval`, `satp`, `menvcfg` and
`senvcfg`, in this order. The field `profile`, which is 2 here, is part
of the hash as before, so no state of this profile hashes like a state of
the trap profile. Hashes of the report and trap profiles are those of
0.5.0.

`rr_boot` resets a machine of the system profile to the reset state of
that profile. Two commands run a flat image in it, with 65536 bytes of
memory:

| Command | What it does |
|---|---|
| `emu-rv64 srun IMAGE STEPS` | As `run`, and two more lines: the mode, `satp`, `scause`, `stval` and `sepc`, then `x10` to `x17`. |
| `emu-rv64 sreplay IMAGE STEPS` | As `replay`. |

`run` and `replay` keep the trap profile. The count of interrupts that
`srun` prints takes the cause from `scause` when the trap went to S-mode.

## Flat image

A flat image is a file of raw bytes. It has no header, no load address and
no entry point of its own. Byte `i` of the image goes to address
`0x80000000 + i`. Every byte of memory after the image is zero, also when
memory held other bytes before the load. An image whose length is not a
multiple of 4 is legal: the zero fill completes its last word.

`img_load(img, n, ram)` loads the first `n` bytes of the slice `img`. It
works on a byte slice and reads no file, so the checks call it without one.

| Code | Name | When | Memory after the call |
|---|---|---|---|
| 0 | `IMG_OK` | 1 <= `n` <= `ram.len()` | The image, then zeros |
| 1 | `IMG_EMPTY` | `n` = 0 | Unchanged |
| 2 | `IMG_BIG` | `n` > `ram.len()` | Unchanged |

`rr_boot` makes the start state of a run. It loads the image and then
resets everything else: the machine to the reset state of the trap profile
(`pc` = `0x80000000`, `mstatus` = `0x1800`, `mtime` = 0, `mtimecmp` all
ones, every other CSR 0, both counters 0, no reservation), every register
to 0, every byte of the output buffer to 0, and the tape, described below,
to empty. When
the loader refuses the image, `rr_boot` changes nothing.

Only the command line reads a file (`src/main4.bld`). It refuses three
files, each with its own message and exit status:

| File | Message | Exit status |
|---|---|---|
| Missing, or it cannot be opened | `error: cannot read image FILE` | 3 |
| 0 bytes long | `error: image FILE is empty` | 4 |
| Longer than memory | `error: image FILE has N bytes and memory has M` | 5 |

The commands give the machine 256 bytes of memory, the size the timer
program of 0.3.0 ran in, or 65536 bytes when the last argument says so. The
output buffer holds 128 bytes.

## Running an image

One step of a run is one call of `emu_step` that returns `EMU_OK` or
`EMU_TRAP`. A program may send more bytes than the output buffer holds.
When the buffer is full the core returns `EMU_SMALL` and changes nothing.
The run then empties the buffer and steps again, and the caller counts one
step.

The tape is what a run keeps beside the machine:

| Field | Meaning |
|---|---|
| `sent` | Number of bytes emptied from the output buffer so far. |
| `dig` | FNV-1a hash of those bytes, in the order they were sent. |
| `irqs` | Number of interrupts taken. It is printed and is not part of the state hash; `traps` in the machine counts the same events together with the synchronous traps. |

Emptying the buffer adds its bytes to `dig`, adds their count to `sent`,
sets every byte of the buffer to 0 and sets `out_len` to 0. The number of
bytes a program has sent is `sent` + `out_len`.

## State hash

The state hash is FNV-1a at 64 bits. It starts at `0xCBF29CE484222325`.
For each byte it takes the exclusive or with the byte and multiplies by
`0x100000001B3` modulo 2^64. A 64-bit word goes in as 8 bytes, least
significant first. The parts go in this order:

1. The 32 registers, `x0` first.
2. The fields of the machine: `pc`, `steps`, `out_len`, `profile`, `traps`,
   `mstatus`, `mtvec`, `mepc`, `mcause`, `mtval`, `mie`, `mip`, `mscratch`,
   `mtime`, `mtimecmp`.
3. Every byte of memory, lowest address first.
4. Every byte of the output buffer, used or unused.
5. Only when `sent` is greater than 0: `sent` as a word, then `dig` as a
   word.

Since 0.5.0 the fields of the machine in part 2 end with two more words,
only while a reservation is held: its width, then its address. A state
with no reservation hashes as it did in 0.4.0.

Parts 1 to 4 are the hash of 0.3.0, unchanged. A run that has sent at most
128 bytes hashes exactly as it did in 0.3.0. The hash covers all of memory
and all of the buffer, so the same image in 65536 bytes of memory has other
hashes than in 256 bytes.

Taking a hash reads the state and changes nothing.

## Replay

A checkpoint is the state hash recorded at a fixed step of a run. A run of
`S` steps records one after every 1000 steps, and one at step `S` when `S`
is not a multiple of 1000. A run of 0 steps records one, the hash of the
loaded state.

`replay IMAGE STEPS` does this:

1. Run 1 starts from `rr_boot`, runs `STEPS` steps and records its
   checkpoints.
2. Run 2 starts from `rr_boot` again, in the same arrays that run 1 left
   behind, and records its checkpoints.
3. The two lists are compared from the first entry on. The runs agree when
   the lists have the same length and every entry is equal.

The command prints each checkpoint once, as `step N hash=H`. When the runs
agree it prints `runs agree at K of K checkpoints` and exits with status 0.
At the first entry that differs it prints the step and the two hashes, then
`RUNS DISAGREE, first at step N`, and exits with status 1. It prints no
checkpoint after that one.

Agreement shows that this build of the emulator, on this host, reached the
same state at each checkpoint in two runs inside one process. It says
nothing about whether those states are the right ones; the hand-computed
cases, the reference model and the fuzz runs are the checks for that. A
difference that appears and is gone again between two checkpoints is not
seen.

## Image and replay commands

| Command | What it does |
|---|---|
| `emu-rv64 run IMAGE STEPS [RAM]` | Loads the file, runs `STEPS` steps and prints the number of interrupts, instructions retired and traps, `pc`, `mtime`, the number of bytes sent to the serial device, and the state hash. |
| `emu-rv64 replay IMAGE STEPS [RAM]` | Runs the file twice as the section "Replay" says. |
| `emu-rv64 image NAME` | Prints a built-in image as hexadecimal digits, 16 bytes to a line. `NAME` is `timer` or `trap`. |
| `emu-rv64 image NAME FILE` | Compares `FILE` with the built-in image and prints `equal=true` or `equal=false`. Exit status 1 when they differ. |

Since 0.5.0 `run` and `replay` execute M and A instructions, because they
use the trap profile. Since 0.6.0 `srun` and `sreplay` do the same in the
system profile; the section "State hash, images and commands in the
system profile" describes them.

`RAM` is `256` or `65536`. The commands print the length of the serial
output and never its bytes.

The two built-in images are programs of earlier versions, laid out by the
loaders those versions used:

| Name | Bytes | Program |
|---|---|---|
| `timer` | 88 | The timer program of `src/iprog.bld`: main code in bytes 0 to 51, zeros in bytes 52 to 63, the handler in bytes 64 to 87. |
| `trap` | 84 | The trap-handler program of `src/mprog.bld`: main code in bytes 0 to 31, zeros in bytes 32 to 63, the handler in bytes 64 to 83. |

The serial program of `cases` is not offered as an image. It ends with
EBREAK and was written for the report profile, where EBREAK stops the run.
In the trap profile EBREAK traps to `mtvec` = 0, which is outside memory,
and every later step would take an instruction access fault.

The package does not write an image file. The standard library function
`write_bytes` takes its data as a string, and no way was found to build a
string that holds a zero byte or one byte above `0x7F`: a string literal
ends at its first zero byte, and a character above `0x7F` becomes two
bytes. So `image NAME` prints hexadecimal text and a host tool turns it
into bytes. `tests/timer.img` was made with

    emu-rv64 image timer | xxd -r -p > tests/timer.img

and `emu-rv64 image timer tests/timer.img` confirms that the file holds the
built-in bytes. Reading is exact: `read_bytes` returns every byte of the
file, zero bytes included.

## How the contract is checked

No other emulator, no assembler of another project and no test suite was
used, as a reference or in any other way. Every expected value comes from
the published text, worked by hand or computed by a second implementation
written from the same text, with two exceptions in the cases of 0.4.0: the
two state hashes that 0.3.0 printed for the timer program, and three FNV-1a
values from memory. The host tool `xxd` turned hexadecimal text into the
test image files; it ran no program of the machine.

For the M and A extensions there are three implementations of the rules,
and they share no code:

| Implementation | Where | How it is built |
|---|---|---|
| The core | `src/lib6.bld`, `src/lib7.bld` | 32-bit halves for the upper product, the unsigned division of the host on magnitudes, named fields for the reservation. |
| The reference model | `src/xmodel.bld`, `src/xmodel2.bld` | Rows of mask and value for the words, a 128-bit product by shift and add, restoring division one bit per turn, the reservation in two slots of the model array, memory one byte per turn. |
| The oracle script | `tests/tools/ma_oracle.py` | Python, standard library only. The encodings are the rows of the listing typed as bit strings, and the arithmetic uses integers that have no width. It computed every word and every expected value of the hand cases. |

The property rules are a fourth statement of the M rules: they multiply on
16-bit digits and judge division by the identity quotient x divisor +
remainder = dividend, with no model.

For the system profile there are again three implementations of the
rules, and they share no code:

| Implementation | Where | How it is built |
|---|---|---|
| The core | `src/sys1.bld` to `src/sys5.bld` | `mstatus` as one word with masks, the walk as a loop over the levels with shifts, an access as at most two parts, one per page. |
| The reference model | `src/smodel.bld` to `src/smodel4.bld` | Every field of `mstatus` in a slot of its own, the walk as a function that calls itself for the next level, the address put together field by field, every byte of an access translated by itself. |
| The oracle script | `tests/tools/sv39_oracle.py` and the six modules beside it | Python, standard library only. The fields of `mstatus` and `sstatus` are the field lists of the manual's figures, typed in the order of the figure; the encodings are rows of the listings typed as bit strings; the walk follows the ten numbered steps. It computed every word and every expected value of the hand cases. |

| Check | Command | What it does |
|---|---|---|
| Hand-computed cases, report profile | `emu-rv64 cases` | The 61 single instructions and two programs of 0.1.0. |
| Hand-computed cases, trap profile | `emu-rv64 mcases` | 59 cases: 13 CSRRW, 9 CSRRS, 4 CSRRC, 9 immediate forms, 12 trap causes, 5 trap entry rules, 4 MRET, 2 report profile, 1 WFI (a no-op since 0.3.0). Each word was assembled by hand and the working is in `src/mvec.bld` and `src/mvec2.bld`. Then one program of 13 hand-assembled words (`src/mprog.bld`): it installs a handler in `mtvec`, executes ECALL, the handler adds 4 to `mepc` and returns with MRET, and the program sends one byte to the serial device. |
| Hand-computed cases, timer and interrupts | `emu-rv64 icases` | 56 cases: 10 timer loads, 4 accesses that leave a register, 6 timer stores, 8 on the pending bits, 7 on the three conditions of delivery, 6 on priority, 5 on the handler address and `mepc`, 4 on WFI and the tick order, 5 sequences of two or three steps, 1 with a short register array. Each word was assembled by hand and the working is in `src/ivec.bld`, `src/ivec2.bld` and `src/ivec3.bld`. Then the timer program below. |
| Differential fuzz | `emu-rv64 fuzz SEED STEPS`, `emu-rv64 mfuzz SEED STEPS`, `emu-rv64 ifuzz SEED STEPS` | A seeded stream of instruction words runs through the core and through the reference model. Result code, `pc`, step count, trap count, all 32 registers, all of memory, the serial output and, in `mfuzz` and `ifuzz`, all eight CSRs, `mtime` and `mtimecmp` are compared after every step. The `mfuzz` stream is 5 Zicsr, MRET or WFI words in 16, 1 MRET, 1 ECALL or EBREAK, 2 random words and 7 RV64I instructions; three CSR numbers in four name an implemented CSR; `mtvec` and `mip` are reset from the stream at fixed intervals. Since 0.3.0 that stream also takes software and external interrupts. It never touches the timer. The `ifuzz` stream replaces 6 words in 16 of the `mfuzz` stream: 2 loads or stores at offsets -2 to 9 of a timer register, with values near the present time, 1 write to `mie`, 1 set or clear of MIE, 1 WFI and 1 MRET. The host flips MSIP or MEIP every eight steps. The run prints one counter for each synchronous cause and each interrupt cause. |
| Properties | `emu-rv64 props SEED STEPS`, `emu-rv64 mprops SEED STEPS`, `emu-rv64 iprops SEED STEPS` | Decoding an encoded instruction gives the instruction back. Encoding a decoded random word gives the word back; in `mprops` the random words all carry the SYSTEM opcode. Then the hand-computed cases. `iprops` adds a timer round trip (a store, then a load of the same width at the same offset) and five rules judged at every step of the `ifuzz` stream: no interrupt is taken that the state before the step did not permit, a permitted one is taken, the entry state is right, the handler address is right, and `mtime` rises by exactly 1. No model is used. `tests/props.txt` lists every kind. |
| Controls | `emu-rv64 controls`, `emu-rv64 mcontrols`, `emu-rv64 icontrols` | Copies of the core, each with one planted defect: six for each profile and seven for the timer and the interrupts. Both checks must refuse every one. For controls 13 to 19 the command also prints every hand-computed case that fails and the verdict of the five stream rules run alone, and it requires both to refuse. |
| Hand-computed cases, images and replay | `emu-rv64 rcases` | 27 cases: 8 on the loader (byte order, zero fill, a final word cut short, the last legal size, one byte over, an empty image, a second load over a first, a full read back), 2 on the start state of the first and of the second run, 1 with three FNV-1a values, 4 on the parts of the state the hash covers, 4 on the checkpoint compare, and 8 runs of the two built-in images. The working is in `src/rvec.bld`, `src/rvec2.bld` and `src/rvec3.bld`. |
| Properties, images and replay | `emu-rv64 rprops SEED STEPS` | Eight rules judged on seeded random images, with no model. The section "Loader and replay checks" lists them, and `tests/props.txt` gives their kinds, 52 to 59. |
| Hand cases, M and A | `emu-rv64 xcases` | 398 cases. 229 run one M instruction: each of MUL, MULH, MULHSU and MULHU on 17 operand pairs, each of DIV, DIVU, REM and REMU on 21, MULW on 9 and each of DIVW, DIVUW, REMW and REMUW on 17, with division by zero and the overflow pair at both widths among them. 167 run one to four words with a doubleword of memory: LR and SC in both widths with and without a reservation, at another address, of the other width, misaligned, outside memory and at the serial device; sequences of LR, SC and AMO; every AMO in both widths on four operand pairs; register overlaps; the ordering bits; atomics on the timer registers; and the read of `misa`. 2 run an M word and an A word in the report profile. The words and the expected values were computed by `tests/tools/ma_oracle.py`, and `src/xvec.bld` explains the layout of a case. |
| Differential fuzz, M and A | `emu-rv64 xfuzz SEED STEPS` | The `mfuzz` stream with 11 words in 16 replaced: 5 M instructions on operands drawn from 0, 1, -1, the most negative and the most positive number, the numbers around 2^31 and 2^32, small numbers and noise, and 6 atomic instructions. Of the atomic words 2 in 8 are LR, 2 are SC and 4 are AMOs; half of the SC words take the address and width of the reservation the core holds. Of 16 addresses 9 are one of two doublewords, 2 are misaligned, 1 is a timer register, 2 lie at the end of memory and 2 are drawn more widely. Everything `ifuzz` compares is compared after every step, and the reservation too. Since 0.5.0 `mfuzz` and `ifuzz` compare the reservation as well. |
| Properties, M and A | `emu-rv64 xprops SEED STEPS` | Fourteen rules judged once per round with no model, kinds 70 to 83 of `tests/props.txt`: two round trips, five rules on the M results, five on the atomic instructions, the value of `misa`, and that the state hash covers the reservation. Each rule runs single instructions through the step function and looks at registers, memory and the trap registers. |
| Controls, M and A | `emu-rv64 xcontrols` | Eleven planted defects, numbered 27 to 37. The differential check, the property rules and the hand cases must each refuse every one. |
| The program `mulatomic` | `emu-rv64 run tests/mulatomic.img 100` | A program of 50 words that multiplies, divides, takes a remainder and adds to a doubleword in an LR and SC loop whose first SC fails. It checks its own results and sends one byte when they are right. `asm-rv64` 0.2.0 assembled it from `tests/mulatomic.s` of that package, where the expected registers are worked out by hand. |
| Controls, images and replay | `emu-rv64 rcontrols` | Seven planted defects, numbered 20 to 26: three in the loader, two in the state hash, one in the start state of the second run and one in the checkpoint compare. The hand-computed cases must refuse each one, and the property rules, run without the cases, must refuse each one. A number passed down selects the defect; with the number 0 every function calls the core directly, and the commands `run` and `replay` use that same path. |
| Hand cases, system profile | `emu-rv64 scases` | 875 generic cases and the 3456 rows of the permission matrix, all computed by the oracle script. The generic cases: 218 on the CSRs (71 that write all ones or zero to a register and read it back, 23 on `mstatus` and `sstatus`, 36 on `sie`, `sip`, `mie` and `mip`, 22 on `satp` and TVM, 66 on the access rules in each mode), 33 on SRET, MRET, WFI, SFENCE.VMA, ECALL and EBREAK in each mode with TSR, TW and TVM clear and set, 48 on the status stack at trap entry and at the two returns, 104 that raise each synchronous cause in each mode with the cause delegated, not delegated, and every other cause delegated, 156 on the six interrupts in each mode with MIE and SIE clear and set, 75 on priority, 10 on vectoring, 7 sequences, and 224 on translation: 54 on the shape of the walk and on superpages, 108 on the flags, SUM, MXR and MPRV, 11 on addresses and on devices behind a page, 27 on accesses across two pages and 24 on the atomic instructions. A row of the matrix is one fetch, load or store of one page: six machine modes (U, S, M, and M with MPRV and MPP = U, S or M) x SUM x MXR x the eight values of R, W and X x U x three states of A and D x three kinds of access. `src/svec.bld` and `src/svec2.bld` explain the layout. |
| Differential fuzz, system profile | `emu-rv64 sfuzz SEED STEPS` | A seeded stream in 32768 bytes of memory, eight pages. Every 256 steps the stream writes fresh page table entries: seven times in eight a gigapage that maps memory for the supervisor and one that maps it for the user, and drawn entries elsewhere, among them entries that are not valid, pointers with a reserved flag, misaligned superpages, entries with a reserved bit, and entries that name a frame outside memory or a device. Every 32 steps it draws the whole state: the mode, every writable bit of `mstatus`, `satp` (12 of 16 on the root table, 2 with MODE 0, 1 with another table as root, 1 with a root outside memory), `medeleg`, `mideleg`, `mie`, `mip`, the vectors, the saved pcs and the pc. Twice in a round, one time in two, the host moves the machine to a drawn mode. The words: of 16, 3 Zicsr on a drawn CSR, 2 of the PRIV group, 4 loads or stores at drawn virtual addresses (one in four in the last eight bytes of a page), 2 atomic, 1 M, 1 noise, 3 RV64I. The result code, the pc, the counters, the timer, the reservation, the mode, every CSR, all 32 registers and the serial output are compared after every step. All of memory is compared after every eighth step and after the last one. |
| Properties, system profile | `emu-rv64 sprops SEED STEPS` | Fifteen kinds of rule, 90 to 104 of `tests/props.txt`, judged on the stream of `sfuzz` with no model. The section "Rules of the system profile" lists them. |
| Controls, system profile | `emu-rv64 scontrols` | Twenty planted defects, numbered 38 to 57. The command runs the hand cases, the differential check and the property rules on each and prints which of the three refuse it. |
| The program `sv39` | `emu-rv64 srun tests/sv39.img 200` | A program of 75 instruction words in an image of 4132 bytes. Machine code builds three page tables and delegates causes 8 and 13, then drops to S-mode. The supervisor turns paging on and enters U-mode. The user program makes an environment call, stores and loads in its own page, and loads from a page that is not mapped. The supervisor handles both traps and stops in a loop. `asm-rv64` 0.3.0 assembled it from `tests/sv39.s` of that package, where the expected end state is worked out by hand. |

The reference model shares no code with the core. For RV64I
(`src/model.bld`, `src/model2.bld`) it matches a word against a table of mask
and value pairs, builds immediates one bit at a time and adds with a ripple
carry. For the trap profile (`src/mmodel.bld`) it keeps the CSRs in an array
of slots where the core has named fields, holds MIE and MPIE as two separate
slots and builds `mstatus` from them on a read, sets and clears CSR bits one
at a time, and finds a fault address by walking the bytes of the access.

For the timer and the interrupts (`src/imodel.bld`) the model keeps `mtime`
and `mtimecmp` in two slots of its array where the core has fields of the
machine. It decides "timer pending" from the borrow of the subtraction
`mtime - mtimecmp`, worked one bit at a time, where the core compares. It
reads and writes a timer register one byte at a time and finds each byte by
walking the sixteen byte addresses. It scans a table of causes in priority
order, and it adds the cause to BASE four times for a vectored address.

For the M and A extensions (`src/xmodel.bld`, `src/xmodel2.bld`) the
model is built as the table above says. It extends both operands of a
product to 128 bits, so the signed forms need no correction, and it tests
the two fixed cases of division as rows of the manual's table before any
arithmetic, also at 32 bits, where the core lets them fall out of the
64-bit routine.

The six model files call no function of the core. This was checked by
listing every call in them: each is a `ref_` function defined in a model
file, or the built-in `len`. What the two sides share is data: the
`Machine` record for `pc` and the counters, and the names of the model
slots. The harness around them uses core helpers to build the stream and to
write a word into memory.

The same person wrote the core and the model from the same text, so a
misreading of the text can sit in both. The hand-computed cases are the guard
against that.

A weakness the method had until 0.4.0 was that one author wrote the core,
the model and the cases, in one language. The oracle script is the answer
this version gives: a second language, unbounded integers, and no shared
table. It is still by the same author, from the same reading of the text.

`tests/controls.txt` lists the planted defects and the check that refuses
each. A check that accepts a planted defect is a broken check, and the
controls commands exit with status 1 in that case.

The reference model of the system profile calls no function of the five
core files. This was checked by listing every call in `smodel.bld` to
`smodel4.bld`: each is a `sref_` function of those files, a `ref_`
function of the older model files, or the built-in `len`. It is a second
implementation of the whole profile and not an extension of the older
models: it has its own CSR table, trap entry, interrupt scan, walk and
memory access, and it takes from the older files the arithmetic of RV64I
and of the M extension and the helper that mixes an AMO.

The harness around the two uses core helpers to build the stream. One
needs saying: to place the word of a step at the pc, the harness asks the
core's own translation for the physical address. A wrong translation of
the pc would put the word where the model does not fetch, and the two
would then run different words, which the comparison sees.

Unlike the controls of 0.1.0 to 0.5.0, which are copies or wrappers of
the core, the twenty defects of the system profile live inside the core:
every function of `sys1.bld` to `sys5.bld` takes a variant number, and a
defect is a branch on it. `emu_step` passes 0. This puts test branches
into the code that runs a program. It was chosen because most of the
defects sit in the middle of the walk, where no wrapper reaches.

### Rules of the system profile

`sprops` runs the stream of `sfuzz` through the core alone. The rules own
one piece of machinery, `sp_leaf` in `src/sprops.bld`: it follows the
pointers of the page tables for an address and returns the PTE the walk
must end at, with its level. It decides nothing about permission. Kinds
91, 93 to 95, 97 to 100 and 103 are judged at every step, from the state
before it and the state after it. Kinds 90, 92, 96 and 104 are judged
before every sixteenth step.

| Kind | Rule |
|---|---|
| 90 | With `satp` MODE 0, and in M-mode without MPRV whatever `satp` holds, the translation of a drawn address is the address itself, for each kind of access. |
| 91 | A step that trapped changed no register, no byte of memory (by a hash of all of it), no byte of output, not the reservation, not `mtimecmp` and no CSR outside the trap registers of the mode that took the trap. |
| 92 | The walk reads nothing but the PTEs on its path. The PTEs on the path of a drawn address are copied into a second memory that is otherwise a fixed pattern, and the translation there gives the same result. |
| 93 | An instruction that retired below M-mode with translation on was fetched through a canonical address whose path ends in an aligned leaf with X and A set, and with U set exactly when the mode is U. |
| 94 | A load, store or atomic instruction that retired through translation used a canonical address whose path ends in an aligned leaf with A set; in U-mode with U set, in S-mode with U clear or SUM set; for a load with R, or X under MXR; for a store with W and D. Judged for the page of the first byte. |
| 95 | A synchronous trap goes to S-mode exactly when its `medeleg` bit is set and the machine was not in M-mode, and no trap lowers the mode. |
| 96 | In S-mode, `sstatus` reads `mstatus` and `0x2000C0122`, `sie` reads `mie & mideleg` and `sip` reads `mip & mideleg`. |
| 97 | An interrupt is taken exactly when the state before the step permits one; the one taken is permitted; it goes to S-mode exactly when it is delegated; and none permitted for M-mode is passed over for one for S-mode. |
| 98 | An instruction that retired was allowed in its mode: MRET only in M, SRET not in U nor in S under TSR, WFI not below M under TW, SFENCE.VMA not in U nor in S under TVM, a Zicsr word only on a CSR whose number allows the mode and not on `satp` in S under TVM. After SRET and MRET the mode, the enable, the stack, MPRV and the pc are those the rule of the return gives. |
| 99 | After every step `satp` is 0 or has MODE 8 with no ASID bit, `mstatus` has no bit outside its writable fields but UXL = SXL = 2 and MPP is not 2, the delegation and enable registers hold only bits that exist, the saved pcs are multiples of 4 and the mode is 0, 1 or 3. |
| 100 | Causes 4, 5 and 13 are taken only by a load or LR, and causes 6, 7 and 15 only by a store, SC or AMO. |
| 103 | In M-mode no fetch takes a page fault, whatever MPRV holds. |
| 104 | A load in S-mode from a drawn page: an execute-only page faults with cause 13 while MXR is clear and gives the data while it is set; a readable user page faults while SUM is clear and gives the data while it is set. |

Kinds 101 and 102 are not used.

### The timer program

`src/iprog.bld` holds a program of 19 hand-assembled words. It installs a
handler in `mtvec`, stores 100 to `mtimecmp`, enables the timer interrupt
and loops. The handler adds 100 to `mtimecmp`, sends the byte `.` to the
serial device, counts in `x12` and returns with MRET. The run is M = 10000
steps, and the number of interrupts N was worked out by hand before the run:

- `mtime` starts at 0 and rises by 1 at the end of every step, so step `s`
  starts with `mtime` = `s` - 1.
- Steps 1 to 11 are the eleven setup instructions. MIE is 1 from step 12 on.
- The first step that starts with `mtime` >= 100 is step 101. It takes the
  interrupt.
- The handler runs in steps 102 to 107, and step 104 stores 200 to
  `mtimecmp`. Nothing is pending again until step 201.
- Deliveries fall on steps 100 k + 1. 100 k + 1 <= 10000 holds for k = 1 to
  99, so N = 99.

The same working gives the end state: 99 traps, 10000 - 99 = 9901
instructions retired, `mtime` = 10000, `mtimecmp` = 100 + 99 x 100 = 10000,
`x12` = 99 and 99 bytes of output. Of the 9901 instructions, 11 are setup
and 6 x 99 = 594 are handler, so the loop retired 9296, half of them its
ADDI: `x10` = 4648. The command checks each of these numbers.

The program runs twice. After each run a hash of the whole state is taken:
the 32 registers, every field of the machine, every byte of memory and
every byte of output, through FNV-1a at 64 bits. The two hashes must be
equal. A third run of 9999 steps must give a different hash, which shows
that the hash depends on the state.


### Loader and replay checks

`rprops` makes one image per round from the seed and stops when the first
runs of the rounds add up to `STEPS` steps. The images come in four kinds,
in equal shares:

- random bytes;
- words of the `mfuzz` stream;
- a layout that installs a trap handler at `0x80000080`, where the handler
  steps over the instruction that trapped and returns, so a run goes on
  after a trap;
- the timer image with a random tail.

Lengths run from 1 to 256 bytes, and one image in eight of the first two
kinds has all 256. A round runs 1 to 400 steps, one round in eight up to
2500, with a checkpoint interval of 1, 7, 100 or 1000 steps. All runs of
all rounds use one set of arrays, so every load meets the memory an earlier
run left.

| Kind | Rule |
|---|---|
| 52 | After a load, byte `i` of memory is byte `i` of the image below the image length, and zero from there on. |
| 53 | An image of 0 bytes and one of 257 bytes are refused with codes 1 and 2, and memory is unchanged. Judged in one round in eight. |
| 54 | Two runs of the image from scratch give equal checkpoint lists. |
| 55 | A run that records no hash before the end ends with the hash of the run that recorded one at every interval. Taking a hash changes nothing. |
| 56 | One flipped bit in one image byte changes the hash of the loaded state. |
| 57 | The same flipped bit changes the hash of the end state, unless the program overwrote that byte in one of the two runs. |
| 58 | A change to one part of an end state changes its hash. The part is a register, `mtime`, another field of the machine, a byte of memory, a byte of the output buffer, or the count of emptied bytes. |
| 59 | A checkpoint list compared with a copy of itself shows no difference. Compared with a copy that has one changed entry, the first difference is at that entry. |

Case 23 of `rcases` runs the timer image twice and, in the second run,
flips bit 0 of `x11` after step 1950. The handler loads `x11` again in step
2002. The compare must report the first difference at step 2000 while the
two final hashes are equal. The case also prints what `replay` prints for
such a pair of runs. Control 26, which compares only the last hash, fails
this case.

### The timer image

`tests/timer.img` is the built-in image `timer`: 88 bytes, SHA-256
`f908cafcb509b05f81d785db80c34a7cb374b3e817100f6bc72bde476655b9b6`. The
repository keeps every file under `registry/` byte for byte, with no line
ending conversion, and the package checksum covers the file.

`emu-rv64 run tests/timer.img 10000` takes 99 interrupts and prints the
hash `f1b4f1df12342aae`. That is the hash 0.3.0 printed for the same
program placed in memory word by word. The two are equal because the
loaded state is equal: the same bytes in 256 bytes of memory with zeros
after the program, the same reset state, the same 128-byte output buffer,
and fewer than 129 bytes sent. In 65536 bytes of memory the same run ends
with the hash `97c89756f2969aae`.

## Limits

Limits of the system profile, added in 0.6.0:

- One hart. Nothing here exercises two harts, an interrupt sent from one
  hart to another, or a page table changed by another hart during a
  walk.
- One author wrote the core, the reference model, the property rules,
  the controls, the oracle script and the test program, from one reading
  of the manual. A misreading can sit in all of them. The oracle is a
  second language and shares no code or table with the package, but it
  is the same reading. The script was run with one Python, 3.12, on the
  same host.
- No architecture test has been run, and no other emulator, assembler or
  fuzzer was used or downloaded. The only reference is the text of the
  manual. No kernel has run on the machine.
- The system profile is a third profile beside the trap profile, which
  stays a machine with M-mode alone. Another design would have given
  the trap profile itself the two modes, as 0.5.0 gave it the M and A
  extensions. The third profile was chosen so that the 34 commands of 0.5.0
  print the same bytes and the checks of 0.2.0 to 0.5.0 keep judging the
  machine they were written for.
- No translation cache, so a program that forgets SFENCE.VMA cannot fail
  here. No physical memory protection: its CSRs do not exist, and S-mode
  and U-mode reach every physical address when translation is off.
- A and D follow the Svade scheme: the machine never sets either bit,
  and a kernel must set them or handle the page fault. Software written
  for hardware that sets the bits itself takes page faults here that it
  does not take there.
- Absent, and reported as absent where the manual gives a way: the
  hypervisor extension, Sstc (no `stimecmp`; STIP comes from machine-mode
  software), Svnapot, Svpbmt, Svinval, Svadu, the counters `cycle`,
  `time` and `instret` and their enable bits, and every device but the
  serial device and the timer. `mcycle` and `minstret`, which the manual
  lists for M-mode, have been absent since 0.2.0 and still are.
- Choices of this machine that software can see, each marked "a choice"
  in the tables above: an access to a CSR that does not exist traps; a
  write of 2 to MPP is ignored; a write of `satp` MODE 0 clears the whole
  register; no ASID bit; the machine interrupts cannot be delegated; WFI
  traps below M-mode under TW and never waits; a pointer PTE with D, A
  or U set faults; a store across two pages changes nothing when either
  page faults; for an access across two pages both pages are translated
  before either part is checked against physical memory, so a page fault
  of the second page comes before an access fault of the first; the
  reservation holds a physical address; a trap and a
  return leave the reservation; MPRV is honoured in every mode.
- The prose of the chapters does not give the field positions. They
  came from figure files of the manual: the fields of `mstatus` and `sstatus`, the three fields of
  `satp`, the fields of a virtual address, a physical address and a PTE,
  the interrupt bits of `mip` and `sip`, and the encodings of SRET, MRET,
  WFI and SFENCE.VMA. The section "Sources" lists the files. Everything
  else was read from the text of the chapters.
- Not confirmed from the fetched text: that a pointer PTE with D, A or U
  set must fault (the text calls the bits reserved and step 3 of the walk
  faults on reserved bits, but no sentence joins the two); the trap value
  of an access fault raised for a PTE outside memory (the virtual address
  of the access is used); and the exact meaning of MPRV when it is set
  below M-mode, which the architecture cannot reach.
- The hand cases cover the 32-bit atomic forms and the instructions of
  the M extension in the system profile only through the fuzz. Their
  rules are those of 0.5.0 and their code is shared with the trap
  profile.
- In the fuzz, U-mode is entered often and left fast: about one step in
  sixteen begins in U-mode and about one in eight of those retires an
  instruction, because most drawn words trap there. The stream sits in
  M-mode for more than half of its steps.
- The fuzz compares all of memory after every eighth step only. A
  difference in memory that appears and is gone again
  within eight steps is not seen. Registers and CSRs are compared after
  every step.
- The property rules judge the page of the first byte of a data access.
  The second page of an access that crosses is covered by the hand cases
  and by the differential check only.
- The planted defects are branches inside the core, as the section "How
  the contract is checked" says. A reader who wants the machine without
  them has to read past them.
- The image `tests/sv39.img` was written by the host tool `xxd` from the
  hexadecimal text `asm-rv64 bytes` prints.
- The fuzz memory is 32768 bytes and the commands `srun` and `sreplay`
  use 65536. Other sizes were not run.

Limits of the M and A extensions, added in 0.5.0:

- One hart. The atomic instructions are exercised on one hart only, so no
  interleaving of two harts is tested, and nothing here shows that the
  instructions are atomic. On this machine they cannot be anything else.
  The ordering bits `aq` and `rl` have no effect.
- One author wrote the core, the reference model, the property rules, the
  controls and the oracle script, from one reading of the manual. A
  misreading can sit in all of them. The script was run with one Python,
  3.12, on the same host.
- No architecture test has been run, and no other emulator or assembler
  was used. The package was built and run on one Windows x86-64 host with
  one C compiler.
- The reservation rules marked "a choice" in the section "Reservation" are
  choices of this machine. Software that relies on an SC of the other
  width succeeding, or on a trap ending the reservation, sees another
  result here than on a machine that chose differently.
- A misaligned atomic address always takes the address-misaligned cause,
  also when the address is outside memory.
- The cause numbers 4 and 6 were read from a table of the privileged
  manual that lists names and numbers in two parallel columns. The names
  "Load address misaligned" and "Store/AMO address misaligned" are the
  fifth and the seventh name, and the numbers were taken by position.
- The M and A extensions exist in the trap profile only and cannot be
  switched off: `misa` ignores writes.
- Five outputs of earlier commands changed. `mcases` prints the new value
  of `misa` in cases 9 and 17. `mfuzz 1 20000`, `mfuzz 7 50000`,
  `ifuzz 7 50000` and the first block of `mcontrols` print other counts,
  because their streams hold random words, and a few of those are now M or
  A instructions that execute where 0.4.0 took an illegal-instruction
  trap. In `mfuzz 1 20000` it is one word: cause 2 falls from 4420 to
  4419, and the word traps with one of the two new causes, 4 or 6, which
  these commands do not print. The other 21 commands of 0.4.0 print the
  same bytes. The report profile is the way to the old behaviour: it has
  no M and no A.
- The same holds for an image: a program that relied on an M or A word
  being illegal runs differently. `tests/timer.img` has none, and its
  hashes are those of 0.4.0.
- The state hash covers the reservation only while one is held.
- The expected values of the A cases include choices of this machine: the
  tick of `mtime`, the order of the two checks and the reservation rules.
  The oracle script marks each as a choice.
- For this version more files of the manual were fetched than the M and
  the A chapter and their figures: the instruction listing, the atomics
  examples, the two chapters the A chapter is made of, and the
  machine-level chapter. The section "Sources" lists every file.

Limits of the loader and of replay, added in 0.4.0:

- Replay shows that this emulator repeats itself on one host. It does not
  compare the emulator with hardware or with another emulator, and it was
  run on one Windows x86-64 host only. The two runs happen inside one
  process.
- A checkpoint is taken every 1000 steps. A difference between two runs
  that appears and is gone again between two checkpoints is not seen.
- The loader, the hash, the replay check, the hand-computed cases, the
  property rules and the controls have one author. The loader and the
  replay check have no reference model.
- The only disagreement `replay` has ever printed is the one case 23 makes
  on purpose. No run of the command on a file has disagreed, so that branch
  of the command is exercised only through the case.
- Rule 57 is weaker than its wording suggests. With a correct loader, equal
  end hashes mean equal memory, so the rule can refuse only a hash that
  misses a difference in memory. It does not show that a changed
  instruction changes what the program does.
- About half of the steps of an `rprops` run are traps: most random
  images reach an illegal word early and then trap again at every step.
  Timer interrupts are rare in it, about 2 in 1000 steps.
- Memory is 256 or 65536 bytes in the commands, and the output buffer is
  128 bytes. The core takes slices of any length; other sizes were not run
  from a file.
- A flat image only. There is no ELF loader, no load address and no entry
  point other than the start of memory.
- The commands print the length of the serial output and its part in the
  hash, never the bytes.
- The package cannot write an image file, for the reason the section "Image
  and replay commands" gives. `tests/timer.img` was written by the host tool
  `xxd` from the hexadecimal text the package prints.
- `irqs`, the count of interrupts, is printed and is not part of the hash.
- The three FNV-1a values of case 11, for the empty input, `a` and
  `foobar`, are from memory, like the two constants. The implementation
  gives all three.
- 0.4.0 adds no rule of the instruction set, and nothing was read from the
  manual for it. The machine-level chapter was fetched once to confirm that
  its SHA-256 is still the one recorded below.

Limits carried over from 0.3.0:

- The rules that 0.3.0 adds were first written from memory. They were then
  compared with `src/priv/machine.adoc` at commit
  `d50ad50f7dfad97a47a70ae0fab6a11660362167` of `riscv/riscv-isa-manual`
  (file SHA-256 `ed164558403bba4e05a14bfceba5b0940fd109ac280b2c13b5ea9cdd87531ade`).
  The text states each of these, and the document agrees:
  - the priority order of the machine interrupts: external, software, timer;
  - the cause numbers 3, 7 and 11, and that bit _i_ of `mip` and `mie`
    belongs to interrupt _i_;
  - the Interrupt bit of `mcause` is set for an interrupt;
  - `mepc` holds the instruction that was interrupted, and `mtval` is 0;
  - an interrupt is taken in M-mode only while MIE is 1 and the bit is set in
    both `mip` and `mie`;
  - in vectored mode an interrupt goes to BASE + 4 x cause and a
    synchronous exception goes to BASE;
  - MTIP is read-only in `mip` and is pending while `mtime >= mtimecmp`,
    compared as unsigned integers;
  - WFI may be implemented as a no-op.
- One thing is still from memory and unconfirmed: the CLINT addresses
  `0x02004000` and `0x0200BFF8` of the QEMU `virt` machine. The manual
  gives `mtime` and `mtimecmp` no address. The WFI word `0x10500073` was
  confirmed for 0.6.0 from the figure that lists the privileged
  instructions.
- The comparison was a reading of the text by the author of the core. No
  second reader has checked it, and no architecture test has been run.
- The hand-computed cases, the model and the property rules were written by
  the same author as the core. They show that the core does what this
  document says.
- WFI does not wait. A run of idle steps costs one step each.
- No interrupt controller. MSIP and MEIP are set by the host only: the
  timer block has no `msip` register, and there is no PLIC, the platform
  interrupt controller that would drive MEIP.
- `mtime` counts steps. It has no relation to seconds.
- The report and trap profiles have machine mode only. The system
  profile adds the other two modes, virtual memory and delegation. No
  profile has physical memory protection.
- In the trap profile software cannot set a bit of `mip`.
- The `mfuzz` stream takes software and external interrupts and never the
  timer interrupt. The `ifuzz` stream takes all three.
- The state hash of the timer program uses the FNV-1a constants
  `0xCBF29CE484222325` and `0x100000001B3`, and checks itself on the value
  `0xAF63DC4C8601EC8C` for the one byte `a`. All three are from memory. An
  error in them would not weaken the check, which only needs equal states
  to hash equal and the shorter run to hash differently.
- No floating-point or compressed instructions.
- One processor.
- The fuzz memory is 1024 bytes. Larger memories use the same code paths; they
  have not been run under the fuzz.
- The official RISC-V architecture tests have not been run, on this version
  or on any earlier one. Their sources use macros and directives that
  `asm-rv64` does not have.
- The interpreter decodes every instruction each time it runs. No speed is
  claimed.
- The rules of 0.2.0 that were taken from memory were confirmed for
  0.6.0: the `funct3` values of the six Zicsr instructions (1, 2, 3, 5,
  6, 7) and the SYSTEM opcode `0x73` from the rows of the instruction
  listing, and the MRET word `0x30200073` from the figure that lists the
  privileged instructions. The manual says an access to a CSR that does
  not exist is reserved; trapping with cause 2 is the choice made here.

## Verified

Every line of the first two lists below was run on the 0.6.0 build, on
one Windows x86-64 host. "Accepted" means the run ended with no
difference from the model, or with no rule broken. The limit beside all
of it: these runs check the core against this document, with checks
written by the author of the core.

Checks added in 0.6.0:

- `scases`: 875 of 875 generic cases and 3456 of 3456 rows of the
  permission matrix pass. Of the rows, 3188 expect a fault and 268 expect
  the access to succeed. The oracle script, run again, wrote the 38 table
  files with the bytes that are committed (compared with `cmp`). It
  checks itself too: the field list of `sstatus` lies inside that of
  `mstatus` field for field, each row of its listing decodes to itself,
  the four words of the PRIV group have the values of the listing figure,
  and every matrix row is asserted against the state of its own machine
  (the page unchanged and `x10` 0 after a fault, the trap value equal to
  the address).
- `sfuzz`: accepted at every seed, 18,070,000 steps in all. "Begun" counts the
  steps that started in each mode and "Retired" the instructions retired
  there. "Fetches" and "Data" count retired instructions whose fetch, and
  whose load, store or atomic access, went through the page tables. The
  causes and the interrupts are counted whichever mode took them, and "In
  S" counts all traps taken in S-mode.

  | Seed | Steps | Begun U | S | M | Retired U | S | M | Fetches | Data | MRET | SRET | CSR | Cause 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 11 | 12 | 13 | 15 | Int. 1 | 3 | 5 | 7 | 9 | 11 | In S |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | 1 | 20000 | 1289 | 6874 | 11837 | 170 | 2146 | 6305 | 1951 | 666 | 335 | 451 | 1853 | 66 | 1056 | 2617 | 316 | 49 | 1575 | 167 | 2006 | 10 | 104 | 210 | 1290 | 455 | 600 | 91 | 123 | 93 | 359 | 67 | 125 | 4486 |
  | 7 | 50000 | 3165 | 15874 | 30961 | 337 | 4550 | 16625 | 4064 | 1424 | 930 | 1144 | 4782 | 179 | 2714 | 6347 | 814 | 156 | 4067 | 419 | 5042 | 11 | 222 | 570 | 3544 | 910 | 1179 | 190 | 347 | 182 | 1022 | 194 | 379 | 10639 |
  | 3 | 2000000 | 127262 | 665758 | 1206980 | 15526 | 207307 | 642082 | 188889 | 67839 | 36589 | 43690 | 187603 | 7306 | 102707 | 259411 | 32027 | 5152 | 161184 | 15940 | 200594 | 907 | 9018 | 22350 | 127804 | 42993 | 55769 | 7866 | 13002 | 6709 | 43243 | 7230 | 13873 | 431859 |
  | 9 | 2000000 | 127488 | 669646 | 1202866 | 15481 | 206820 | 638525 | 188080 | 67142 | 36490 | 43393 | 187044 | 7291 | 103658 | 259882 | 31926 | 5345 | 159957 | 15951 | 200146 | 951 | 8925 | 22344 | 132327 | 42801 | 56140 | 7296 | 13007 | 6951 | 43010 | 7440 | 13826 | 437020 |
  | 4 | 2000000 | 127498 | 661283 | 1211219 | 15488 | 203728 | 643540 | 185252 | 66502 | 36538 | 43316 | 187338 | 7316 | 101578 | 258122 | 32164 | 5307 | 160992 | 15916 | 200913 | 971 | 8957 | 22538 | 131319 | 42824 | 55983 | 7428 | 13030 | 7031 | 43575 | 7306 | 13974 | 430945 |
  | 5 | 2000000 | 127291 | 660561 | 1212148 | 15147 | 205683 | 643500 | 186565 | 66739 | 36559 | 43657 | 187861 | 7253 | 105011 | 258708 | 32384 | 5290 | 161038 | 15902 | 201518 | 906 | 8923 | 22306 | 126048 | 42357 | 55303 | 7582 | 13216 | 6853 | 43084 | 7555 | 14433 | 428017 |
  | 11 | 10000000 | 637789 | 3321032 | 6041179 | 76985 | 1025342 | 3208947 | 931357 | 331914 | 181564 | 217629 | 937175 | 36531 | 520370 | 1293419 | 161209 | 26675 | 805772 | 79916 | 1002505 | 4518 | 44741 | 111930 | 649820 | 213269 | 278383 | 37243 | 66794 | 34026 | 216194 | 36331 | 69080 | 2164686 |

- `sprops`: accepted at seed 1 with 20000 steps, seed 7 with 50000, seed
  2 with 1,000,000 and seed 3 with 2,000,000.
- `scontrols` (seed 1, 20000 steps for the differential check and for
  the rules): the core accepted by all three checks, and each of the
  twenty planted defects refused by all three. The last column says
  whether the hand cases, the differential check and the rules refused
  the control.

  | Control | Planted defect | Differential refusal | Property rule | Hand cases that fail | Cases, fuzz, rules |
  |---|---|---|---|---|---|
  | 38 | the canonical test of a virtual address is missing | kind 1 at step 774 | kind 94 at step 589 | 9, first 653 | yes, yes, yes |
  | 39 | a misaligned superpage is accepted | kind 1 at step 578 | kind 94 at step 561 | 9, first 659 | yes, yes, yes |
  | 40 | the U bit of a page is ignored | kind 2 at step 96 | kind 104 at step 3 | 192, first 729 | yes, yes, yes |
  | 41 | SUM is ignored: S-mode may always read and write a user page | kind 1 at step 740 | kind 104 at step 3 | 49, first 729 | yes, yes, yes |
  | 42 | MXR is ignored: an execute-only page is never readable | kind 1 at step 3595 | kind 104 at step 3 | 21, first 734 | yes, yes, yes |
  | 43 | a PTE with W and without R is accepted as a leaf | kind 1 at step 1096 | kind 94 at step 265 | 81, first 742 | yes, yes, yes |
  | 44 | the walk has two levels | kind 33 at step 12 | kind 104 at step 3 | 1799, first 380 | yes, yes, yes |
  | 45 | the walk has four levels | kind 33 at step 12 | kind 104 at step 3 | 1106, first 386 | yes, yes, yes |
  | 46 | the A bit is ignored | kind 1 at step 3943 | kind 94 at step 3786 | 122, first 725 | yes, yes, yes |
  | 47 | the D bit is ignored | kind 1 at step 11813 | kind 94 at step 5480 | 44, first 762 | yes, yes, yes |
  | 48 | MPRV also applies to the fetch | kind 32 at step 33 | kind 103 at step 192 | 803, first 667 | yes, yes, yes |
  | 49 | a delegated trap goes to S-mode also when taken in M-mode | kind 2 at step 34 | kind 95 at step 67 | 11, first 307 | yes, yes, yes |
  | 50 | SRET returns to S-mode whatever SPP holds | kind 30 at step 227 | kind 98 at step 111 | 12, first 230 | yes, yes, yes |
  | 51 | TSR is ignored | kind 1 at step 1810 | kind 98 at step 275 | 1, first 231 | yes, yes, yes |
  | 52 | TVM is ignored | kind 1 at step 912 | kind 98 at step 368 | 3, first 150 | yes, yes, yes |
  | 53 | TW is ignored | kind 1 at step 528 | kind 98 at step 645 | 2, first 224 | yes, yes, yes |
  | 54 | a delegated interrupt is blocked in U-mode while SIE is clear | kind 2 at step 940 | kind 97 at step 1216 | 6, first 408 | yes, yes, yes |
  | 55 | sstatus reads all of mstatus | kind 3 at step 422 | kind 96 at step 3 | 17, first 27 | yes, yes, yes |
  | 56 | satp accepts a write with an unsupported MODE | kind 37 at step 14376 | kind 99 at step 4907 | 9, first 47 | yes, yes, yes |
  | 57 | a store page fault is reported with cause 13 | kind 2 at step 21 | kind 100 at step 259 | 789, first 395 | yes, yes, yes |

  The line the command ends with: `result: core accepted, 20 of 20 controls refused by all three checks, 0 by one or two`.
  The hand cases are thinnest for controls 51, 53 and 52, which fail 1, 2 and 3 of them.
- `srun tests/sv39.img 200` (first line `run image=tests/sv39.img bytes=4132 ram=65536 steps=200`) prints the values worked
  out by hand in `tests/sv39.s` of `asm-rv64` 0.3.0: 198 instructions
  retired, 2 traps, the loop of the supervisor at `0x80000100`, mode S,
  `scause` 13, `stval` `0x80006018`, `sepc` `0x8000101C`, and `x10` to
  `x17`. The lines it prints after the first:

      interrupts=0 retired=198 traps=2 pc=80000100 mtime=200 serial_len=0
      mode=1 satp=8000000000080002 scause=d stval=80006018 sepc=8000101c
      x10=2a x11=7 x12=7 x13=80006018 x14=2a x15=80006018 x16=d x17=8000101c
      hash=e15fb2f971dfeb00

  The file has 4132 bytes and the SHA-256
  `82d30101dd65995fd8f7d79428b387f8ce6df2c73c2563b4acb2b0819b7f25d7`.
- `sreplay tests/sv39.img 200`: the two runs agree at 1 of 1 checkpoints.
  `sreplay tests/sv39.img 5000`: runs agree at 5 of 5 checkpoints, and the last line before it is
  `step 5000 hash=3537068c02481e44`.
- `run tests/sv39.img 200 65536`, the same image in the trap profile: 25
  instructions retired and 175 traps. The write to `medeleg` is illegal
  there and traps to `mtvec` = 0, outside memory.

Checks of 0.1.0 to 0.5.0, run again on the 0.6.0 build:

- The 34 commands that `tests/cases.txt` listed for 0.5.0 print the bytes
  of the 0.5.0 goldens. The 34 golden files are byte for byte those of
  0.5.0.
- Seven long runs were repeated and gave the counts 0.5.0 recorded, which
  are in the tables further down: `fuzz 3 2000000`, `mfuzz 3 2000000`,
  `ifuzz 3 2000000`, `xfuzz 3 2000000`, `rprops 3 2000000`,
  `xprops 2 1000000` and `iprops 2 1000000`.
- The other rows of the tables further down were not repeated on this
  build.
- `buildc pkg check-registry` accepts the registry with this version in
  it, and the registry package test of the compiler builds the package
  and compares the 43 commands of `tests/cases.txt` with their goldens.

Checks added in 0.5.0, as recorded for 0.5.0:

- `xcases`: 398 of 398 cases pass. The oracle script that computed them
  also checks itself: every row of its listing decodes to itself, the two
  halves of each product recombine to the product of its unbounded
  integers, and each quotient and remainder satisfy the division identity.
- `xfuzz`: accepted at every seed, 18,070,000 steps in all. "M" counts
  retired M instructions, "By zero" and "Overflow" the divisions among
  them that met the two fixed cases, "On timer" the retired atomic
  instructions that named a timer register.

  | Seed | Steps | Retired | M | By zero | Overflow | LR | SC stored | SC failed | AMO | On timer | Cause 4 | 5 | 6 | 7 | Int. 7 |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | 1 | 20000 | 15567 | 6273 | 495 | 2 | 1386 | 471 | 944 | 2775 | 367 | 420 | 398 | 1040 | 368 | 0 |
  | 7 | 50000 | 38559 | 15315 | 1193 | 9 | 3427 | 1149 | 2569 | 6907 | 877 | 984 | 940 | 2693 | 997 | 2 |
  | 3 | 2000000 | 1547636 | 613131 | 46804 | 315 | 137345 | 46069 | 102125 | 276585 | 35402 | 38447 | 38153 | 107065 | 39441 | 474 |
  | 9 | 2000000 | 1548770 | 614508 | 47620 | 322 | 138018 | 46170 | 102279 | 275782 | 34971 | 38348 | 38599 | 106543 | 38974 | 444 |
  | 4 | 2000000 | 1548217 | 614048 | 47256 | 353 | 138016 | 46377 | 102173 | 276405 | 35250 | 38181 | 38373 | 106608 | 39582 | 450 |
  | 5 | 2000000 | 1548825 | 614606 | 47588 | 338 | 138036 | 46006 | 103013 | 275497 | 35581 | 38651 | 38470 | 106219 | 39338 | 462 |
  | 11 | 10000000 | 7739242 | 3070281 | 236432 | 1575 | 689226 | 230995 | 511079 | 1378984 | 176049 | 192555 | 192630 | 532803 | 197521 | 2233 |

- `xprops`: accepted at seed 1 with 20000 rounds, seed 7 with 50000, seed 2
  with 1,000,000 and seed 3 with 2,000,000. A round runs about 45
  instructions.
- `xcontrols` (seed 1, 20000 steps): the core accepted, 11 of 11 planted
  defects refused by all three checks.

  | Control | Planted defect | Differential refusal | Property rule | Hand cases that fail |
  |---|---|---|---|---|
  | 27 | A division by zero gives the quotient 0 | register at step 107 | 75 at round 0 | 22, first 73 |
  | 28 | The most negative number divided by -1 gives the largest positive number | register at step 1948 | 75 at round 0 | 3, first 78 |
  | 29 | A remainder takes the sign of the divisor | register at step 132 | 74 at round 0 | 9, first 112 |
  | 30 | MULHSU reads `rs2` as signed | register at step 158 | 73 at round 0 | 6, first 37 |
  | 31 | A 32-bit result is not sign-extended | register at step 24 | 76 at round 0 | 60, first 154 |
  | 32 | SC succeeds with no reservation | register at step 1 | 77 at round 0 | 6, first 242 |
  | 33 | A successful SC writes 1 to `rd` | register at step 26 | 77 at round 0 | 18, first 241 |
  | 34 | The reservation survives an SC | reservation at step 26 | 77 at round 0 | 31, first 241 |
  | 35 | AMOMIN compares unsigned | memory at step 11 | 78 at round 15 | 6, first 313 |
  | 36 | An AMO returns the new value | register at step 11 | 79 at round 1 | 72, first 256 |
  | 37 | A misaligned atomic access is carried out | result code at step 20 | 80 at round 0 | 14, first 235 |

  Control 28 fails three hand cases only: DIV of the overflow pair, and
  DIVW of that pair with and without stray upper bits.
- `run tests/mulatomic.img 100`: 100 instructions retired, no trap, `pc` =
  `0x80000078`, one byte sent, hash `9de954947c4d6323`. That `pc` is the
  label `pass`, which the program reaches only when the doubleword holds
  1052 and exactly one SC failed. The file has 200 bytes and the SHA-256
  `e2ea00797c528a290ab27f12d7fdddc8b3fc8c67bccd2964bfb7e2cc2972288d`.
- `replay tests/mulatomic.img 100`: the two runs agree at 1 of 1
  checkpoints. `replay tests/mulatomic.img 5000`: they agree at 5 of 5,
  and the last hash is `5a177a91ef57a513`.

Checks of 0.1.0 to 0.4.0, as recorded for 0.5.0:

- 21 of the 26 commands that `tests/cases.txt` listed for 0.4.0 print the
  same bytes as the 0.4.0 goldens. The other five are the ones "Limits"
  names.
- `fuzz 3 2000000`: accepted, with the counts of 0.4.0
  (ok=1175502 illegal=475999 bus=259233 align=31803 loads=40692 stores=23430 jumps=206159).
- `mcases`: 59 of 59 cases pass. `icases`: 56 of 56. `rcases`: 27 of 27.
  The timer program took 99 interrupts in 10000 steps, and both runs gave
  the hash `f1b4f1df12342aae`.
- `mfuzz`: accepted at every seed. The counts differ from 0.4.0 at every
  seed, for the reason "Limits" gives.

  | Seed | Steps | Retired | CSR | MRET | Cause 0 | 1 | 2 | 3 | 5 | 7 | 11 | Int. 3 | Int. 7 | Int. 11 |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | 1 | 20000 | 11845 | 2392 | 1958 | 342 | 330 | 4419 | 745 | 883 | 546 | 783 | 94 | 0 | 12 |
  | 7 | 50000 | 29291 | 5969 | 5044 | 823 | 955 | 11090 | 1924 | 2386 | 1348 | 1876 | 222 | 0 | 81 |
  | 3 | 2000000 | 1180060 | 241320 | 196678 | 32976 | 35875 | 442700 | 77073 | 91412 | 52194 | 77090 | 8225 | 0 | 2287 |
  | 9 | 2000000 | 1180420 | 241456 | 197292 | 33257 | 36013 | 442567 | 76942 | 91388 | 52204 | 76777 | 7740 | 0 | 2596 |

- `ifuzz`: accepted at every seed, 19,070,000 steps in all. The first row
  is that of 0.4.0. The other rows differ from it by a few counts.

  | Seed | Steps | Retired | CSR | MRET | Cause 0 | 1 | 2 | 3 | 5 | 7 | 11 | Int. 3 | Int. 7 | Int. 11 | Timer |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | 1 | 20000 | 12914 | 3771 | 2346 | 255 | 257 | 2710 | 443 | 1129 | 682 | 456 | 370 | 322 | 462 | 1393 |
  | 7 | 50000 | 32540 | 9405 | 5807 | 590 | 629 | 6703 | 1166 | 2798 | 1613 | 1174 | 875 | 788 | 1121 | 3570 |
  | 1 | 1000000 | 652258 | 187659 | 116258 | 12582 | 13380 | 131854 | 22829 | 55433 | 33124 | 22976 | 17199 | 15990 | 22338 | 69745 |
  | 2 | 2000000 | 1305223 | 375030 | 232609 | 24990 | 26288 | 263781 | 45748 | 110659 | 67130 | 45841 | 34915 | 31204 | 44161 | 139879 |
  | 3 | 2000000 | 1304372 | 374541 | 232755 | 25264 | 26916 | 264126 | 45720 | 110895 | 66850 | 45784 | 34785 | 30860 | 44380 | 139478 |
  | 4 | 2000000 | 1304856 | 374999 | 233292 | 25056 | 26684 | 263498 | 45345 | 110743 | 66804 | 45880 | 34945 | 31193 | 44938 | 139225 |
  | 5 | 2000000 | 1303469 | 374011 | 232970 | 24932 | 26680 | 264161 | 45616 | 111001 | 66482 | 46076 | 35188 | 32047 | 44297 | 139434 |
  | 11 | 10000000 | 6524078 | 1876959 | 1163194 | 125511 | 133575 | 1319017 | 229459 | 553265 | 333375 | 228604 | 173555 | 157801 | 221479 | 696523 |

- `mprops 1 20000`: accepted.
- `iprops`: accepted at seed 1 with 20000 steps (1116 interrupts judged),
  seed 2 with 1,000,000 (54605) and seed 3 with 1,000,000 (55204).
- `mcontrols`, `icontrols` and `rcontrols` (seed 1, 20000 steps): every
  planted defect of 0.2.0 to 0.4.0 is refused, at the steps, cases and
  rounds `tests/controls.txt` records. Those did not move.
- `image timer tests/timer.img`, `run tests/timer.img 10000` and the two
  `replay` commands on that file print the bytes of 0.4.0: 99 interrupts,
  hash `f1b4f1df12342aae`, and `fe036f68e96f06af` after 25000 steps in
  65536 bytes of memory.
- `rprops`: accepted at every seed. The rows of seeds 1, 2 and 7 are those
  of 0.4.0, and the long runs differ by a few counts.

  | Seed | Steps run | Images | Retired | Traps | Interrupts | Checkpoints | Overwritten |
  |---|---|---|---|---|---|---|---|
  | 1 | 20031 | 68 | 9033 | 10998 | 49 | 4712 | 0 |
  | 2 | 20043 | 65 | 12933 | 7110 | 92 | 3042 | 0 |
  | 7 | 50241 | 129 | 25193 | 25048 | 139 | 10239 | 0 |
  | 3 | 2000011 | 5956 | 940587 | 1059424 | 4390 | 550723 | 1 |
  | 4 | 2000315 | 6183 | 947695 | 1052620 | 4285 | 560309 | 0 |
  | 5 | 2000000 | 6000 | 974409 | 1025591 | 4412 | 600972 | 0 |
  | 11 | 20000127 | 60326 | 9473717 | 10526410 | 42200 | 5865324 | 1 |

Not verified: the package was not built or run on a second host or with a
second C compiler, so the claim that results are the same on every host
rests on the integer arithmetic alone. The oracle scripts were not run
with a second Python. No program was run on two harts, because there is
one. No kernel, no architecture test and no program written by anyone
else has run on the system profile.

## Sources

- The RISC-V Instruction Set Manual, Volume I: Unprivileged Architecture and
  Volume II: Privileged Architecture, RISC-V International, CC BY 4.0, read
  from the `riscv/riscv-isa-manual` repository at commit `d50ad50f`. The
  Zicsr rules, CSR numbers, `mstatus`, `mtvec`, `mepc`, `mcause`, `mip` and
  `misa` layouts, the cause numbers, trap entry and MRET come from its text
  and register figures, as read for 0.2.0. For 0.3.0 the interrupt rules
  were compared with its machine-level chapter, as "Limits" says. For 0.4.0
  that chapter was fetched once more and its SHA-256 is the one recorded
  there; 0.4.0 takes no new rule from it.
- For 0.5.0, files of the same repository at the same commit, fetched
  on 2026-11-10 and kept as data:

  | File | SHA-256 | What was read from it |
  |---|---|---|
  | `src/unpriv/m-st-ext.adoc` | `99d3708b844072f626e33b427e7b1825cebc0743efdf6f7a5ca58205dd4c7c42` | Every rule of the M extension, as text, and the table of division by zero and overflow. |
  | `src/unpriv/a-st-ext.adoc` | `1d330a27d199376f2cef10bc1bc40bda9c026823d68175b2c3a77455ce75bb90` | That the A extension is the two below. |
  | `src/unpriv/zalrsc.adoc` | `0b4404c4ae422dd5eed8e7a64feef57fbe2f7760bab375b570bd2580ddb1dd39` | LR and SC, the reservation, alignment, the failure code. |
  | `src/unpriv/zaamo.adoc` | `571894f8071d4a715e9a21eab3ebecf95d24e12ab9490fc3145cf95448b92fcd` | The AMOs, their operators, alignment, the ordering bits. |
  | `src/unpriv/rv-32-64g.adoc` | `21f8d1b70e2064660af53d3971d018049a8a9a212943257a6f3b32e812e9426b` | Every encoding of the section "Encodings", from the rows of the listing. |
  | `src/unpriv/atomics-examples.adoc` | `2528e7adb96287bd93e9e300c78ef94c3739d9ba470d7d6b57d32fc205d48cd2` | The written form of the atomic instructions, for `asm-rv64`. |
  | `src/priv/machine.adoc` | `ed164558403bba4e05a14bfceba5b0940fd109ac280b2c13b5ea9cdd87531ade` | The `misa` bits for A and M, the two misaligned causes, their priority, the reservation at MRET and at reset. |
  | `src/unpriv/images/wavedrom/m-st-ext-for-int-mult.edn` | `c41f402f78c36c4a557a3a825fdabab1abc7ac50d8053b84f87f3ad1adc5d0b3` | Field names only. The figure files of these chapters hold no numbers. |
  | `src/unpriv/images/wavedrom/division-op.edn` | `26ee02e1e89ed00f24eebdf12b034c2305af840c384503687f3666543fc4cc4b` | The same. |
  | `src/unpriv/images/wavedrom/load-reserve-st-conditional.edn` | `701ab6a43702946795ea801bf23232ad28584993edff37aeffd16b12c4a3102d` | The same, and that the `rs2` field of LR is 0. |
  | `src/unpriv/images/wavedrom/atomic-mem.edn` | `55a71bcfbc6e43ac40920ec0c41fafc8d6571c60fd75127b33a167e7a5223310` | The same. |

  No encoding and no rule of the M and A extensions is from memory: each
  was read from the text of these files.
- For 0.6.0, files of the same repository at the same commit, fetched on
  2026-11-10 and kept as data. Quotations in this document leave out the
  markup of the source, write its dashes in number ranges as hyphens and
  its not-equal sign as `!=`.

  | File | SHA-256 | What was read from it |
  |---|---|---|
  | `src/priv/supervisor.adoc` | `80fc178596d15a79de1654d9237606c48e1a2e9cd2f42d8adbcd922397ab59d7` | The supervisor CSRs, the field list of `sstatus`, `satp` and its MODE table, SFENCE.VMA, the PTE flags, the two schemes for A and D, the ten steps of the walk, Sv39. |
  | `src/priv/machine.adoc` | `ed164558403bba4e05a14bfceba5b0940fd109ac280b2c13b5ea9cdd87531ade` | The privilege stack, MPRV, SUM, MXR, TVM, TW, TSR, SXL and UXL, delegation, `mip` and `mie`, the interrupt order, `mcounteren`, `menvcfg`, ECALL, the two returns, WFI, reset, and the sentence on memory operations with no PMP entry. |
  | `src/priv/csrs.adoc` | `a8aff82d214b98b5487a8699d2a671ac8b6236586cb83822c034d7631dbb994d` | The access rules from the bits of a CSR number, and the fifteen CSR numbers. |
  | `src/priv/svadu.adoc` | `baf23f5ee5e72b1063337f943c9f8b07f91c3b704587cd5ab7bf772fcbe0cdb8` | That Svade is in effect when Svadu is absent. |
  | `src/priv/sv.adoc`, `src/priv/ssstrict.adoc`, `src/priv/insns.adoc`, `src/priv/intro.adoc` | `a923d8ac...01bb3`, `8d867db6...a93e2`, `60724767...50124`, `35479982...ded4b` | Read, and no rule was taken from them. |
  | `src/unpriv/rv-32-64g.adoc` | `21f8d1b70e2064660af53d3971d018049a8a9a212943257a6f3b32e812e9426b` | The rows the oracle script types for ADDI, JAL, the loads and stores, the Zicsr instructions, ECALL, EBREAK and the atomic instructions. |
  | `src/priv/images/bytefield/priv-instr-set.edn` | `7574703d12f22d76c096ffd4ebaa6e5cb417e6c22b1c314464fc21855c6c4705` | The encodings of SRET, MRET, WFI and SFENCE.VMA. This also confirms the WFI and MRET words that earlier versions took from memory. |
  | `src/priv/images/wavedrom/mstatusreg.edn` | `38e5bd922c7f8616a39f59c15c84e0b77f07d4dc87ac59b01ed49874f0cd0a05` | The position of every field of `mstatus`. |
  | `src/priv/images/bytefield/rv64satp.edn` | `06f8c1cac1cb1ab3a15fb35452bb2d34cd9e0dbb1102b936cbc444d4376379a8` | MODE in bits 63 to 60, ASID in 59 to 44, PPN in 43 to 0. |
  | `src/priv/images/bytefield/sv39va.edn`, `sv39pa.edn`, `sv39pte.edn` | `db24ffd2...24bb8`, `556fe429...28ddb`, `771ef6c4...dfce` | The three VPN fields, the three PPN fields, and the flags and PPN of a PTE. |
  | `src/priv/images/bytefield/mipreg-standard.edn`, `sipreg-standard.edn` | `bd7c2fb3...fedc`, `faec1b8e...f491a` | The bit of each interrupt in `mip` and `sip`. |
  | `src/priv/images/wavedrom/sfencevma.edn`, `trap-return.edn`, `wfi.edn`, `mm-env-call.edn`, `menvcfgreg.edn`; `src/priv/images/bytefield/sv32pte.edn`, `medeleg.edn`, `mideleg.edn`, `miereg-standard.edn`, `siereg-standard.edn`, `counteren.edn`, `scounteren.edn` | in `SHA256SUMS` beside the files | Field names and widths only. No number was taken from them that is not in the files above. |

  The folder that holds these files also holds a `SHA256SUMS` file with
  every hash in full and a note of the source.
- The two timer addresses are those of the `virt` machine of QEMU, from
  memory. No QEMU source or program was fetched or run.
- FNV-1a is the public-domain hash of Fowler, Noll and Vo. Its constants
  and the three check values are from memory, as "Limits" says.
- The 16550 register layout is the common one for that family of serial
  chips. Only the transmit register and the line status register are modelled.
