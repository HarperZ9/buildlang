# emu-rv64 0.4.0: RV64I machine emulator core with a timer, interrupts, flat images and replay

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

The package holds a decoder (word to operation and fields), an encoder (the
reverse, the seed of an assembler), a step function that executes one
instruction against registers, memory, one serial device and the timer, and
a loader and a replay check for flat images.

The core owns no memory and reads no clock. The caller passes registers,
memory and the serial output buffer in as slices. Time is a count of steps,
so a run repeats exactly. No kernel has run on it yet. The section "Limits"
lists what is missing.

Exactness class: `exact-int`. Every result is an integer. The same starting
state and the same instruction give the same next state on every host, bit for
bit.

## Two profiles

A profile is the rule the step function follows when an instruction cannot
complete. The caller picks one when it makes the machine.

| Constructor | Profile | An instruction that cannot complete |
|---|---|---|
| `emu_new()` | report | The step changes nothing and returns a code that names the reason. This is the 0.1.0 contract, unchanged. Zicsr words and MRET are illegal in this profile. |
| `emu_new_machine()` | trap | The machine takes a trap: it records the cause in its CSRs and moves `pc` to the handler. The step returns `EMU_TRAP`. The timer and the interrupts exist in this profile only. |

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
`mcause`, `mtval` and `mtime`: no register, no byte of memory and no byte of
output.

`emu_run(m, x, ram, out, max)` calls `emu_step` up to `max` times. In the
report profile it stops at the first code other than `EMU_OK` and returns it.
In the trap profile it continues through `EMU_TRAP`, stops only on
`EMU_SMALL`, and returns `EMU_OK` after `max` steps.

Rules the manual leaves to the environment, in both profiles: a load or store
at an address that is not a multiple of its size is allowed; FENCE completes
and does nothing; JALR clears bit 0 of its target and then the alignment rule
applies.

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
| `0x301` | `misa` | `0x8000000000000100`: 64-bit, base set I | Nothing. The write is ignored and is legal. |
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
seven causes in the table below are synchronous, and they go to BASE in both
modes of `mtvec`. Interrupts follow the rules of the section "Interrupts".

| Cause | Name | When | `mtval` |
|---|---|---|---|
| 0 | Instruction address misaligned | A taken jump or branch names a target that is not a multiple of 4. `mepc` is the jump or branch, and no link register is written. Also when the host sets a `pc` that is not a multiple of 4. | The target address |
| 1 | Instruction access fault | Any of the 4 bytes at `pc` is outside memory. | `pc` |
| 2 | Illegal instruction | The word is not an instruction of this machine, names an unknown CSR, or writes a read-only CSR. | The word |
| 3 | Breakpoint | EBREAK | 0 |
| 5 | Load access fault | A load reads a byte outside memory, other than a one-byte load from the serial device. | The first byte address of the access that is outside memory |
| 7 | Store access fault | A store writes a byte outside memory, other than a one-byte store to the serial device. Nothing is stored. | The same |
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
ones, every other CSR 0, both counters 0), every register to 0, every byte
of the output buffer to 0, and the tape, described below, to empty. When
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

No other emulator, assembler or test suite was used, as a reference or in any
other way. Every expected value comes from the published text, worked by hand
or computed by a second implementation written from the same text, with two
exceptions in the cases of 0.4.0: the two state hashes that 0.3.0 printed
for the timer program, and three FNV-1a values from memory. The host tool
`xxd` turned hexadecimal text into the test image file; it ran no program
of the machine.

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
| Controls, images and replay | `emu-rv64 rcontrols` | Seven planted defects, numbered 20 to 26: three in the loader, two in the state hash, one in the start state of the second run and one in the checkpoint compare. The hand-computed cases must refuse each one, and the property rules, run without the cases, must refuse each one. A number passed down selects the defect; with the number 0 every function calls the core directly, and the commands `run` and `replay` use that same path. |

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

The four model files call no function of the core. This was checked by
listing every call in them: each is a `ref_` function defined in a model
file, or the built-in `len`. What the two sides share is data: the
`Machine` record for `pc` and the counters, and the names of the model
slots. The harness around them uses core helpers to build the stream and to
write a word into memory.

The same person wrote the core and the model from the same text, so a
misreading of the text can sit in both. The hand-computed cases are the guard
against that.

`tests/controls.txt` lists the planted defects and the check that refuses
each. A check that accepts a planted defect is a broken check, and the
controls commands exit with status 1 in that case.

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
- Two things are still from memory and unconfirmed:
  - the WFI word `0x10500073`. The manual keeps the encoding in a figure
    file that could not be fetched;
  - the CLINT addresses `0x02004000` and `0x0200BFF8` of the QEMU `virt`
    machine. The manual gives `mtime` and `mtimecmp` no address.
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
- Machine mode only: no supervisor or user mode, no virtual memory, no
  physical memory protection, no interrupt delegation.
- Software cannot set a bit of `mip`.
- The `mfuzz` stream takes software and external interrupts and never the
  timer interrupt. The `ifuzz` stream takes all three.
- The state hash of the timer program uses the FNV-1a constants
  `0xCBF29CE484222325` and `0x100000001B3`, and checks itself on the value
  `0xAF63DC4C8601EC8C` for the one byte `a`. All three are from memory. An
  error in them would not weaken the check, which only needs equal states
  to hash equal and the shorter run to hash differently.
- No multiply and divide, atomic, floating-point or compressed instructions.
- One processor.
- The fuzz memory is 1024 bytes. Larger memories use the same code paths; they
  have not been run under the fuzz.
- The official RISC-V architecture tests have not been run, on this version
  or on any earlier one. They need an assembler for their sources, which
  this package does not have yet.
- The interpreter decodes every instruction each time it runs. No speed is
  claimed.
- Rules of 0.2.0 taken from memory and still not confirmed against the
  text: the `funct3` values of the six Zicsr instructions (1, 2, 3, 5, 6, 7), the
  SYSTEM opcode `0x73`, and the MRET word `0x30200073`. The manual says an
  access to a CSR that does not exist is reserved; trapping with cause 2 is
  the choice made here.

## Verified

Every line below was run on the 0.4.0 build, on one Windows x86-64 host.
The lines that 0.3.0 already listed were run again and gave the same
numbers. "Accepted" means the run ended with no difference from the model,
or with no rule broken. The limit beside all of it: these runs check the
core against this document, with checks written by the author of the core.

Checks of 0.1.0 to 0.3.0:

- The 15 commands that `tests/cases.txt` listed for 0.3.0 print the same
  bytes as the 0.3.0 goldens.
- `cases`, `fuzz 1 20000`, `fuzz 7 50000`, `props 1 20000` and `controls`
  print the same bytes as the 0.1.0 goldens.
- `fuzz 3 2000000`: accepted (ok=1175502 illegal=475999 bus=259233 align=31803 loads=40692 stores=23430 jumps=206159).
- `mcases`: 59 of 59 cases pass, and the trap-handler program ends with 11
  instructions retired, 1 trap, and the byte `0x21` in the output.
- `icases`: 56 of 56 cases pass. The timer program took 99 interrupts in
  10000 steps and ended with 9901 instructions retired, 99 traps, `mtime` =
  `mtimecmp` = 10000, `x10` = 4648, `x12` = 99 and 99 bytes of output. Both
  runs gave the hash `f1b4f1df12342aae`, and the run of 9999 steps gave
  `02a86ecc64904958`.
- `mfuzz`: accepted at every seed. Software and external interrupts are
  taken in this stream; the timer is never touched.

  | Seed | Steps | Retired | CSR | MRET | Cause 0 | 1 | 2 | 3 | 5 | 7 | 11 | Int. 3 | Int. 7 | Int. 11 |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | 1 | 20000 | 11845 | 2392 | 1958 | 342 | 330 | 4420 | 745 | 883 | 546 | 783 | 94 | 0 | 12 |
  | 7 | 50000 | 29284 | 5989 | 5036 | 825 | 953 | 11105 | 1931 | 2379 | 1351 | 1868 | 223 | 0 | 81 |
  | 3 | 2000000 | 1180331 | 241388 | 196842 | 33021 | 35850 | 442961 | 76963 | 91279 | 52128 | 77070 | 8123 | 0 | 2274 |
  | 9 | 2000000 | 1180131 | 241678 | 197372 | 33234 | 36020 | 442933 | 77039 | 91466 | 52077 | 76823 | 7794 | 0 | 2483 |

- `ifuzz`: accepted at every seed, 19,070,000 steps in all. Every
  synchronous cause and every interrupt cause was reached at every seed.
  "Timer" counts loads and stores aimed at a timer register.

  | Seed | Steps | Retired | CSR | MRET | Cause 0 | 1 | 2 | 3 | 5 | 7 | 11 | Int. 3 | Int. 7 | Int. 11 | Timer |
  |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
  | 1 | 20000 | 12914 | 3771 | 2346 | 255 | 257 | 2710 | 443 | 1129 | 682 | 456 | 370 | 322 | 462 | 1393 |
  | 7 | 50000 | 32538 | 9403 | 5808 | 591 | 629 | 6706 | 1166 | 2800 | 1614 | 1174 | 877 | 788 | 1117 | 3571 |
  | 1 | 1000000 | 652167 | 187655 | 116252 | 12592 | 13408 | 131911 | 22833 | 55435 | 33104 | 22988 | 17204 | 15987 | 22371 | 69714 |
  | 2 | 2000000 | 1305362 | 375396 | 232748 | 25020 | 26306 | 264098 | 45499 | 110719 | 66897 | 45708 | 34707 | 31493 | 44191 | 139747 |
  | 3 | 2000000 | 1304330 | 374548 | 232772 | 25266 | 26927 | 264209 | 45734 | 110909 | 66837 | 45776 | 34791 | 30833 | 44388 | 139482 |
  | 4 | 2000000 | 1304817 | 375014 | 233274 | 25074 | 26694 | 263577 | 45354 | 110763 | 66779 | 45900 | 34924 | 31183 | 44935 | 139241 |
  | 5 | 2000000 | 1303508 | 374088 | 232992 | 24949 | 26685 | 264213 | 45602 | 110970 | 66462 | 46083 | 35203 | 32031 | 44294 | 139434 |
  | 11 | 10000000 | 6524106 | 1876941 | 1163070 | 125534 | 133578 | 1319486 | 229439 | 553348 | 333221 | 228519 | 173562 | 157810 | 221397 | 696618 |

- `mprops 1 20000`: accepted.
- `iprops`: accepted at seed 1 with 20000 steps (1116 interrupts judged),
  seed 2 with 1,000,000 (54605) and seed 3 with 1,000,000 (55206). Each
  run includes the round trips of `mprops` at the same seed and count.
- `mcontrols` (seed 1, 20000 steps): the core accepted, 6 of 6 planted
  defects refused by both checks. The steps differ from 0.2.0 because the
  stream now holds WFI words.

  | Control | Planted defect | Differential refusal | Second refusal |
  |---|---|---|---|
  | 7 | A trap saves `pc` + 4 in `mepc` | `mepc` at step 0 | hand case 13 |
  | 8 | ECALL reports cause 8 | `mcause` at step 5 | hand case 36 |
  | 9 | MRET does not restore MIE from MPIE | `mstatus` at step 102 | hand case 53 |
  | 10 | CSRRS with `rs1` = 0 counts as a write | result code at step 15 | hand case 16 |
  | 11 | CSRRC sets bits | `mcause` at step 74 | hand case 23 |
  | 12 | CSR number sign-extended | result code at step 15 | property 42, round 0 |

- `icontrols` (seed 1, 20000 steps): the core accepted, 7 of 7 planted
  defects refused by the differential check, by hand-computed cases and by
  the stream rules run alone.

  | Control | Planted defect | Differential refusal | Hand cases that fail | Stream rule |
  |---|---|---|---|---|
  | 13 | An interrupt is taken while MIE is 0 | `mcause` at step 12 | 29, 49, 51 to 55 | 46 at step 15 |
  | 14 | An interrupt leaves bit 63 of `mcause` clear | `mcause` at step 98 | 31 to 33, 36 to 44, 46, 48, 52, 53, 55 | 47 at step 52 |
  | 15 | An interrupt saves `pc` + 4 in `mepc` | `mepc` at step 98 | the same 17 | 48 at step 52 |
  | 16 | Vectored handler at BASE + cause | `pc` at step 135 | 42, 43, 44 | 49 at step 101 |
  | 17 | Timer pending only while `mtime` > `mtimecmp` | result code at step 900 | 22, 28, 31, 41, 42, 46, 48, 53 | 47 at step 2884 |
  | 18 | Timer interrupt taken before software | `pc` at step 324 | 37, 40 | 46 at step 278 |
  | 19 | `mtime` rises before the interrupt check | `mtime` at step 31 | 9, 19, 20, 21, 50 | 46 at step 2883 |

Checks added in 0.4.0:

- `rcases`: 27 of 27 cases pass.
- `image timer tests/timer.img`: the file has 88 bytes and equals the
  built-in image.
- `run tests/timer.img 10000`: 99 interrupts, 9901 instructions retired, 99
  traps, `pc` = `0x8000002c`, `mtime` = 10000, 99 bytes sent, hash
  `f1b4f1df12342aae`.
- `replay tests/timer.img 10000`: the two runs agree at 10 of 10
  checkpoints.

  | Step | Hash | Step | Hash |
  |---|---|---|---|
  | 1000 | `3da57ed236ce90af` | 6000 | `43af9c4848739f0d` |
  | 2000 | `ccd553a2669cfd76` | 7000 | `86ad2b20cec40eb6` |
  | 3000 | `3bf14c85aa7b48e9` | 8000 | `aeab517475912730` |
  | 4000 | `71e7c54af21f91e4` | 9000 | `af87ab7378e9420f` |
  | 5000 | `6de0c384d48cd6ab` | 10000 | `f1b4f1df12342aae` |

- `replay tests/timer.img 25000 65536`: the two runs agree at 25 of 25
  checkpoints, and the last hash is `fe036f68e96f06af`. The program sends
  249 bytes in this run, so the output buffer is emptied once.
- `replay tests/timer.img 1000000`: the two runs agree at 1000 of 1000
  checkpoints, and the last hash is `aaa47c7303b17c39`.
- Refusals: `run tests/no-such.img 10` exits with status 3,
  `run tests/empty.img 10` with status 4, and `run tests/golden-cases.txt 10`,
  a file of 5633 bytes, with status 5. Each prints its own message.
- `rprops`: accepted at every seed. "Overwritten" counts the rounds in
  which the program wrote over the flipped byte, the exception of rule 57.

  | Seed | Steps run | Images | Retired | Traps | Interrupts | Checkpoints | Overwritten |
  |---|---|---|---|---|---|---|---|
  | 1 | 20031 | 68 | 9033 | 10998 | 49 | 4712 | 0 |
  | 2 | 20043 | 65 | 12933 | 7110 | 92 | 3042 | 0 |
  | 7 | 50241 | 129 | 25193 | 25048 | 139 | 10239 | 0 |
  | 3 | 2000011 | 5956 | 940586 | 1059425 | 4390 | 550723 | 1 |
  | 4 | 2000315 | 6183 | 947695 | 1052620 | 4285 | 560309 | 0 |
  | 5 | 2000000 | 6000 | 974410 | 1025590 | 4412 | 600972 | 0 |
  | 11 | 20000127 | 60326 | 9473749 | 10526378 | 42200 | 5865324 | 1 |

- `rcontrols` (seed 1, 20000 steps): the core accepted, 7 of 7 planted
  defects refused by the hand-computed cases and by the property rules run
  alone. "Round" is the round of the first refusal.

  | Control | Planted defect | Hand cases that fail | Property rule |
  |---|---|---|---|
  | 20 | The loader drops the last byte of the image | 1, 3, 4, 7 to 10, 20 to 27 | 52 at round 0 |
  | 21 | The loader places the image one byte late | 1 to 4, 7 to 10, 20 to 27 | 52 at round 0 |
  | 22 | The loader leaves memory of an earlier run in place | 2, 3, 7, 20 to 24, 26 | 52 at round 1 |
  | 23 | The state hash leaves `mtime` out | 12, 20 to 24 | 58 at round 24 |
  | 24 | The state hash leaves the serial output out | 13, 14, 20 to 24, 26 | 58 at round 1 |
  | 25 | The second run starts with `mtime` = 1 | 10, 21, 23, 25 | 54 at round 0 |
  | 26 | The checkpoint compare looks only at the last hash | 17, 23 | 59 at round 1 |

- `buildc pkg check-registry` accepts the registry with this version in
  it, and the registry package test of the compiler builds the package and
  compares the 26 commands of `tests/cases.txt` with their goldens.

Not verified: the package was not built or run on a second host or with a
second C compiler, so the claim that hashes are the same on every host
rests on the integer arithmetic alone.

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
- The two timer addresses are those of the `virt` machine of QEMU, from
  memory. No QEMU source or program was fetched or run.
- FNV-1a is the public-domain hash of Fowler, Noll and Vo. Its constants
  and the three check values are from memory, as "Limits" says.
- The 16550 register layout is the common one for that family of serial
  chips. Only the transmit register and the line status register are modelled.
