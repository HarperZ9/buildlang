# emu-rv64 0.2.0: RV64I machine emulator core that can take a trap

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

The package holds a decoder (word to operation and fields), an encoder (the
reverse, the seed of an assembler) and a step function that executes one
instruction against registers, memory and one serial device.

The core owns no memory and reads no clock. The caller passes registers,
memory and the serial output buffer in as slices. No interrupt is delivered
and no kernel has run on it yet. The section "Limits" lists what is missing.

Exactness class: `exact-int`. Every result is an integer. The same starting
state and the same instruction give the same next state on every host, bit for
bit.

## Two profiles

A profile is the rule the step function follows when an instruction cannot
complete. The caller picks one when it makes the machine.

| Constructor | Profile | An instruction that cannot complete |
|---|---|---|
| `emu_new()` | report | The step changes nothing and returns a code that names the reason. This is the 0.1.0 contract, unchanged. Zicsr words and MRET are illegal in this profile. |
| `emu_new_machine()` | trap | The machine takes a trap: it records the cause in its CSRs and moves `pc` to the handler. The step returns `EMU_TRAP`. |

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
| `Machine.mstatus`, `mtvec`, `mepc`, `mcause`, `mtval`, `mie`, `mip`, `mscratch` | The CSRs. `mstatus` starts at `0x1800`, the others at 0. |

Every other address is unmapped.

## Step codes

`emu_step(m, x, ram, out)` executes the instruction at `m.pc` and returns a
code. It is total: it returns a code for every input.

| Code | Name | Report profile | Trap profile |
|---|---|---|---|
| 0 | `EMU_OK` | The instruction retired. `pc` moved and `steps` rose by 1. | The same. |
| 1 | `EMU_BREAK` | The instruction is EBREAK. | Not returned. EBREAK traps with cause 3. |
| 2 | `EMU_ECALL` | The instruction is ECALL. | Not returned. ECALL traps with cause 11. |
| 3 | `EMU_TRAP` | Not returned. | A trap was taken. `traps` rose by 1 and `steps` did not move. |
| -1 | `EMU_ILLEGAL` | The word is not an RV64I instruction. | Not returned. Cause 2. |
| -2 | `EMU_BUS` | The instruction, or the data it names, is outside mapped memory. | Not returned. Cause 1, 5 or 7. |
| -3 | `EMU_ALIGN` | `pc`, or the target of a taken jump or branch, is not a multiple of 4. | Not returned. Cause 0. |
| -4 | `EMU_SMALL` | A byte was sent to the serial device and `out` is full. | The same. The host empties `out` and steps again. |

In the report profile a step that returns any code other than `EMU_OK`
changes nothing. In the trap profile the same holds for `EMU_SMALL`, and a
step that returns `EMU_TRAP` changes only `pc`, `traps`, `mstatus`, `mepc`,
`mcause` and `mtval`: no register, no byte of memory and no byte of output.

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
| `0x304` | `mie` | Stored value | `v & 0x888`: the machine software, timer and external enable bits (3, 7, 11). Storage only. |
| `0x305` | `mtvec` | Stored value | `v & ~2`. Bits 1 and 0 are MODE, and only 0 (direct) and 1 (vectored) are legal, so bit 1 is cleared. The rest is BASE, a multiple of 4. |
| `0x340` | `mscratch` | Stored value | `v`, all 64 bits |
| `0x341` | `mepc` | Stored value | `v & ~3`. Instructions are 4 bytes, so the two low bits are always 0. |
| `0x342` | `mcause` | Stored value | `v`, all 64 bits |
| `0x343` | `mtval` | Stored value | `v`, all 64 bits |
| `0x344` | `mip` | `Machine.mip & 0x888` | Nothing. The write is ignored and is legal. The host sets pending bits in `Machine.mip`. |
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

`mtvec` in vectored mode sends interrupts to BASE + 4 x cause. Synchronous
traps, the only kind here, go to BASE in both modes.

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

## How the contract is checked

No other emulator, assembler or test suite was used, as a reference or in any
other way. Every expected value comes from the published text, worked by hand
or computed by a second implementation written from the same text.

| Check | Command | What it does |
|---|---|---|
| Hand-computed cases, report profile | `emu-rv64 cases` | The 61 single instructions and two programs of 0.1.0. |
| Hand-computed cases, trap profile | `emu-rv64 mcases` | 59 cases: 13 CSRRW, 9 CSRRS, 4 CSRRC, 9 immediate forms, 12 trap causes, 5 trap entry rules, 4 MRET, 2 report profile, 1 WFI. Each word was assembled by hand and the working is in `src/mvec.bld` and `src/mvec2.bld`. Then one program of 13 hand-assembled words (`src/mprog.bld`): it installs a handler in `mtvec`, executes ECALL, the handler adds 4 to `mepc` and returns with MRET, and the program sends one byte to the serial device. |
| Differential fuzz | `emu-rv64 fuzz SEED STEPS`, `emu-rv64 mfuzz SEED STEPS` | A seeded stream of instruction words runs through the core and through the reference model. Result code, `pc`, step count, trap count, all 32 registers, all of memory, the serial output and, in `mfuzz`, all eight CSRs are compared after every step. The `mfuzz` stream is 5 Zicsr or MRET words in 16, 1 MRET, 1 ECALL or EBREAK, 2 random words and 7 RV64I instructions; three CSR numbers in four name an implemented CSR; `mtvec` and `mip` are reset from the stream at fixed intervals. |
| Properties | `emu-rv64 props SEED STEPS`, `emu-rv64 mprops SEED STEPS` | Decoding an encoded instruction gives the instruction back. Encoding a decoded random word gives the word back; in `mprops` the random words all carry the SYSTEM opcode. Then the hand-computed cases. No model is used. |
| Controls | `emu-rv64 controls`, `emu-rv64 mcontrols` | Copies of the core, each with one planted defect: six for each profile. Both checks must refuse every one. |

The reference model shares no code with the core. For RV64I
(`src/model.bld`, `src/model2.bld`) it matches a word against a table of mask
and value pairs, builds immediates one bit at a time and adds with a ripple
carry. For the trap profile (`src/mmodel.bld`) it keeps the CSRs in an array
of slots where the core has named fields, holds MIE and MPIE as two separate
slots and builds `mstatus` from them on a read, sets and clears CSR bits one
at a time, and finds a fault address by walking the bytes of the access.

The same person wrote the core and the model from the same text, so a
misreading of the text can sit in both. The hand-computed cases are the guard
against that.

`tests/controls.txt` lists the planted defects and the check that refuses
each. A check that accepts a planted defect is a broken check, and the
controls commands exit with status 1 in that case.

## Limits

- No interrupt is delivered. `mie` and `mip` are storage, and MIE has no
  effect yet. No timer.
- Machine mode only: no supervisor or user mode, no virtual memory, no
  physical memory protection.
- WFI is not implemented and traps as an illegal instruction.
- Software cannot set a bit of `mip`.
- No multiply and divide, atomic, floating-point or compressed instructions.
- One processor.
- The fuzz memory is 1024 bytes. Larger memories use the same code paths; they
  have not been run under the fuzz.
- The official RISC-V architecture tests have not been run. They need an
  assembler for their sources, which this package does not have yet.
- The interpreter decodes every instruction each time it runs. No speed is
  claimed.
- Rules taken from memory and not confirmed against the text this time: the
  `funct3` values of the six Zicsr instructions (1, 2, 3, 5, 6, 7), the
  SYSTEM opcode `0x73`, and the MRET word `0x30200073`. The manual says an
  access to a CSR that does not exist is reserved; trapping with cause 2 is
  the choice made here.

## Verified

Run on the 0.2.0 build, on one Windows x86-64 host.

- `cases`, `fuzz 1 20000`, `fuzz 7 50000`, `props 1 20000` and `controls`
  print the same bytes as the 0.1.0 goldens.
- `fuzz` at seeds 101, 202 and 303, 1,000,000 steps each: accepted.
- `mcases`: 59 of 59 cases pass, and the trap-handler program ends with 11
  instructions retired, 1 trap, and the byte `0x21` in the output.
- `mfuzz` at seeds 101, 202 and 303, 1,000,000 steps each: accepted, no
  difference from the model.

  | Seed | Retired | CSR | MRET | Cause 0 | 1 | 2 | 3 | 5 | 7 | 11 |
  |---|---|---|---|---|---|---|---|---|---|---|
  | 101 | 577896 | 138188 | 104366 | 16565 | 18709 | 237076 | 38781 | 45580 | 26478 | 38915 |
  | 202 | 576861 | 138588 | 104446 | 16847 | 19422 | 238185 | 38372 | 45682 | 26107 | 38524 |
  | 303 | 577132 | 138390 | 104227 | 16506 | 19159 | 238015 | 38796 | 45868 | 25999 | 38525 |

- `mprops 101 1000000`: 1,000,000 round trips and 1,000,000 raw SYSTEM words
  accepted.
- `mcontrols` (seed 1, 20000 steps): the core accepted, 6 of 6 planted
  defects refused by both checks.

  | Control | Planted defect | Differential refusal | Second refusal |
  |---|---|---|---|
  | 7 | A trap saves `pc` + 4 in `mepc` | `mepc` at step 0 | hand case 13 |
  | 8 | ECALL reports cause 8 | `mcause` at step 5 | hand case 36 |
  | 9 | MRET does not restore MIE from MPIE | `mstatus` at step 31 | hand case 53 |
  | 10 | CSRRS with `rs1` = 0 counts as a write | result code at step 15 | hand case 16 |
  | 11 | CSRRC sets bits | `mepc` at step 1 | hand case 23 |
  | 12 | CSR number sign-extended | result code at step 15 | property 42, round 0 |

## Sources

- The RISC-V Instruction Set Manual, Volume I: Unprivileged Architecture and
  Volume II: Privileged Architecture, RISC-V International, CC BY 4.0, read
  from the `riscv/riscv-isa-manual` repository at commit `d50ad50f`. The
  Zicsr rules, CSR numbers, `mstatus`, `mtvec`, `mepc`, `mcause`, `mip` and
  `misa` layouts, the cause numbers, trap entry and MRET come from its text
  and register figures.
- The 16550 register layout is the common one for that family of serial
  chips. Only the transmit register and the line status register are modelled.
