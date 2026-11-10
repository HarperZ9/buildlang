# emu-rv64 0.1.0: RV64I machine emulator core

An emulator is a program that behaves like a computer: it reads the machine
code of that computer and carries out each instruction. This package is the
core of one for RISC-V, an open instruction set. It covers RV64I, the 64-bit
base integer set: 52 instructions, 32 registers of 64 bits, and a program
counter (`pc`), the address of the next instruction.

The package holds three things:

- a decoder, which turns a 32-bit instruction word into an operation number
  and its fields;
- an encoder, the reverse, which is the seed of an assembler;
- a step function, which executes one instruction against registers, memory
  and one serial device.

The core owns no memory and reads no clock. The caller passes registers,
memory and the serial output buffer in as slices. No kernel has run on it yet,
and the smallest machine that can take an interrupt needs more than this slice
has. The section "Limits" lists what is missing.

Exactness class: `exact-int`. Every result is an integer. The same starting
state and the same instruction give the same next state on every host, bit for
bit.

## Machine

| Part | Model |
|---|---|
| Registers | `x`: caller slice of 32 words. `x[0]` reads 0 and ignores writes. |
| Memory | `ram`: caller slice of bytes at addresses `0x80000000` to `0x80000000 + ram.len() - 1`. Little-endian. A read or write may start at any byte address. |
| Serial device | Eight byte-wide registers at `0x10000000` to `0x10000007`, laid out like the common 16550 serial chip. A byte stored to offset 0 is appended to `out`. A load from offset 5, the line status register, gives `0x60`, which means "ready to send". A load from any other offset gives 0, and a store to any other offset has no effect. Only one-byte accesses reach the device. |
| `Machine.pc` | Address of the next instruction. Starts at `0x80000000`. |
| `Machine.steps` | Number of instructions completed. |
| `Machine.out_len` | Number of bytes written to `out`. |

Every other address is unmapped.

## Step

`emu_step(m, x, ram, out)` executes the instruction at `m.pc` and returns a
code. It is total: it returns a code for every input and never traps.

| Code | Name | Meaning |
|---|---|---|
| 0 | `EMU_OK` | The instruction completed. `pc` moved and `steps` rose by 1. |
| 1 | `EMU_BREAK` | The instruction is EBREAK. |
| 2 | `EMU_ECALL` | The instruction is ECALL. |
| -1 | `EMU_ILLEGAL` | The word is not an RV64I instruction. |
| -2 | `EMU_BUS` | The instruction, or the data it names, is outside mapped memory. |
| -3 | `EMU_ALIGN` | `pc`, or the target of a taken jump or branch, is not a multiple of 4. |
| -4 | `EMU_SMALL` | A byte was sent to the serial device and `out` is full. |

A step that returns any code other than `EMU_OK` changes nothing: no
register, no byte of memory, no byte of output, not `pc`, not `steps`.

Rules the instruction-set manual leaves to the environment, and the choice
made here:

- ECALL and EBREAK stop the machine and report. Trap handling inside the
  machine arrives with the control and status registers.
- A load or store at an address that is not a multiple of its size is
  allowed, as on a machine that supports misaligned access.
- FENCE completes and does nothing, which is correct for one processor with
  no cache model.
- JALR clears bit 0 of its target, as the manual requires, and then the
  alignment rule applies.

`emu_run(m, x, ram, out, max)` calls `emu_step` until a step returns a code
other than `EMU_OK` or `max` steps have completed, and returns the last code.

`emu_decode(w)` returns an `Insn` with operation number 0 when `w` is not an
RV64I instruction. `emu_encode(d)` returns the word for a decoded instruction,
or 0 when `d` names no instruction. Operation numbers are listed in
`src/abi.bld`.

## How the contract is checked

No other emulator was used, as a reference or in any other way. Every
expected value comes from the published instruction-set text, worked by hand
or computed by a second implementation written from the same text.

| Check | Command | What it does |
|---|---|---|
| Hand-computed cases | `emu-rv64 cases` | 61 single instructions. Each word was assembled by hand from the instruction formats and each expected result was worked out on paper. Then two short programs: one prints `OK` to the serial device, one sums 10 down to 1 in a loop. |
| Differential fuzz | `emu-rv64 fuzz SEED STEPS` | A seeded stream of instruction words, three valid in four and one random, runs through the core and through the reference model. Result code, `pc`, step count, all 32 registers, all of memory and the serial output are compared after every step. |
| Properties | `emu-rv64 props SEED STEPS` | Decoding an encoded instruction gives the instruction back. Encoding a decoded random word gives the word back. Then the hand-computed cases. No model is used. |
| Controls | `emu-rv64 controls` | Six copies of the core, each with one planted defect. Both checks must refuse every one. |

`src/model.bld` and `src/model2.bld` are the reference model. It shares no
code with the core. It recognises an instruction by comparing the word with a
table of mask and value pairs, where the core walks the opcode and function
fields. It builds immediates one bit at a time, adds with a ripple carry,
shifts one place at a time and compares by subtraction. It is slow and simple
on purpose.

The same person wrote the core and the model from the same text, so a
misreading of the text can sit in both. The hand-computed cases are the guard
against that, and they cover every one of the 52 instructions at least once.

The generator is splitmix64 in wrapping 64-bit arithmetic, so one seed gives
one instruction stream on every host.

`tests/controls.txt` lists the planted defects and the code that refuses each.
A check that accepts a planted defect is a broken check, and
`emu-rv64 controls` exits with status 1 in that case.

## Limits

- User-level RV64I only. No control and status registers, no traps, no
  interrupts, no timer, no privilege levels, no virtual memory.
- No multiply and divide, atomic, floating-point or compressed instructions.
- One processor.
- The fuzz memory is 1024 bytes. Larger memories use the same code paths; they
  have not been run under the fuzz.
- The official RISC-V architecture tests have not been run. They need an
  assembler for their sources, which this package does not have yet.
- The interpreter decodes every instruction each time it runs. No speed is
  claimed.

## Sources

- The RISC-V Instruction Set Manual, Volume I: Unprivileged Architecture,
  RISC-V International, CC BY 4.0. Instruction formats, immediate encodings
  and the meaning of each RV64I instruction come from its text.
- The 16550 register layout is the common one for that family of serial
  chips. Only the transmit register and the line status register are modelled.
