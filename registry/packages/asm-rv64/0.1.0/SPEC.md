# asm-rv64 0.1.0: RV64I text assembler

An assembler turns a program written as text, one instruction to a line,
into the machine code a processor runs. This package is one for RISC-V, an
open instruction set. It covers what the core of `emu-rv64` 0.4.0 executes:

- RV64I, the 64-bit base integer set, 52 instructions;
- Zicsr, the six instructions that read and write control and status
  registers (CSRs), the registers that hold the state of the processor;
- MRET, the return from a trap handler, and WFI, "wait for interrupt".

The output is a flat image: raw bytes with no header, the form `emu-rv64`
loads at the start of memory. Each instruction is one word of 32 bits,
stored least significant byte first.

The package also holds a renderer, the reverse direction: it writes the
text of an instruction word. The command `dis` prints it.

Exactness class: `exact-int`. The same text gives the same words on every
host. The package reads no clock and draws no random number; the seed of a
property run is an argument.

## One encoder

The assembler holds no bit layout. It reads a statement into the decoded
record of `emu-rv64` (operation number, `rd`, `rs1`, `rs2`, immediate) and
calls `emu_encode` of that package, the encoder its own checks already
exercise. The renderer calls `emu_decode` the same way. So there is one
encoder and one decoder for the emulator and the assembler.

`asm-rv64` reaches them through a registry dependency, `emu-rv64 = "=0.4.0"`.
A dependency joins every library file of the other package in front of
this one. Three consequences:

- `emu-rv64` 0.4.0 is used as published. No byte of it changes.
- Its library files call one function, `show_refusal`, that lives in its
  `src/main.bld`, and a dependent package does not receive that file.
  `src/main.bld` of this package defines the function with the same body.
- The executable carries the whole emulator. The commands `match` and
  `acases` use it to compare an assembled program with the built-in images
  of the emulator.

## Source text

A source file is ASCII text of at most 65536 bytes. A line ends at a line
feed. A carriage return counts as a space, so files with either line
ending assemble alike.

    line       = [ label ":" ] [ statement ] [ "#" comment ]
    statement  = mnemonic [ operand { "," operand } ]
               | ".word" number
               | ".org" number
    operand    = register | number | number "(" register ")" | label | csr
    register   = "x0" ... "x31"
    number     = [ "+" | "-" ] ( digit { digit } | "0x" hexdigit { hexdigit } )
    label      = ( letter | "_" ) { letter | digit | "_" }

- A comment runs from `#` to the end of the line. Blank lines are allowed.
- Spaces or tabs separate a mnemonic from its first operand, and may stand
  around every operand, comma, colon and bracket. A directive whose number
  starts with a sign is also read without the separation (`.word+7`).
- Mnemonics, register names and CSR names are lower case only. Labels keep
  their case: `loop` and `Loop` are two names.
- Registers are `x0` to `x31`. The names of the calling convention (`ra`,
  `sp`, `a0`) are not accepted. `x07` is not a register name.
- A decimal number may have leading zeros and is never read as octal. Hex
  digits may be in either case. The prefix is `0x`; `0X` is refused. A
  number whose magnitude does not fit 64 bits is refused.
- A label is 1 to 31 characters. A line holds at most one label. A label
  may stand alone on its line and then names the next word.
- There are no pseudo-instructions: `nop`, `li`, `mv`, `j` and `ret` are
  unknown mnemonics. Write `addi x0, x0, 0` or `jal x0, target`.

## Statements

Every instruction takes 4 bytes. `rd` is the register written, `rs1` and
`rs2` the registers read.

| Mnemonics | Operands | Number operand |
|---|---|---|
| `add` `sub` `sll` `slt` `sltu` `xor` `srl` `sra` `or` `and` `addw` `subw` `sllw` `srlw` `sraw` | `rd, rs1, rs2` | none |
| `addi` `slti` `sltiu` `xori` `ori` `andi` `addiw` | `rd, rs1, imm` | -2048 to 2047 |
| `slli` `srli` `srai` | `rd, rs1, shamt` | 0 to 63 |
| `slliw` `srliw` `sraiw` | `rd, rs1, shamt` | 0 to 31 |
| `lb` `lh` `lw` `ld` `lbu` `lhu` `lwu` | `rd, imm(rs1)` | -2048 to 2047 |
| `jalr` | `rd, imm(rs1)` | -2048 to 2047 |
| `sb` `sh` `sw` `sd` | `rs2, imm(rs1)` | -2048 to 2047 |
| `beq` `bne` `blt` `bge` `bltu` `bgeu` | `rs1, rs2, target` | -4096 to 4094, even |
| `jal` | `rd, target` | -1048576 to 1048574, even |
| `lui` `auipc` | `rd, imm20` | 0 to 1048575 |
| `fence` | `rd, rs1, bits` | 0 to 4095 |
| `csrrw` `csrrs` `csrrc` | `rd, csr, rs1` | CSR number 0 to 4095 |
| `csrrwi` `csrrsi` `csrrci` | `rd, csr, uimm` | `uimm` 0 to 31 |
| `ecall` `ebreak` `mret` `wfi` | none | none |

Notes on single forms:

- `imm20` of `lui` and `auipc` is the upper 20 bits as a number. The lower
  12 bits of the value are zero and are not written. A negative number is
  refused, so the highest value is written `0xFFFFF`.
- `sltiu` takes -2048 to 2047 like `slti`. The processor sign-extends the
  immediate and then compares without sign.
- A `target` is a label, or a number: the offset in bytes from the branch
  or jump itself. An operand that starts with a digit or a sign is a
  number. `jal x0, 0` jumps to itself. An offset must be even. An offset
  that is even and not a multiple of 4 is accepted, because the format
  holds it; when such a jump is taken, the emulator traps in its trap
  profile and returns the alignment step code in its report profile.
- `fence` is written with all three fields, because the decoder of
  `emu-rv64` accepts every value of them. `bits` is the 12-bit field that
  holds the mode and the two sets. The usual form `fence iorw, iorw` is
  refused. Its word is written `fence x0, x0, 0x0ff`.
- `csr` is a number, or one of the names of the CSRs the core implements:
  `mstatus` (0x300), `misa` (0x301), `mie` (0x304), `mtvec` (0x305),
  `mscratch` (0x340), `mepc` (0x341), `mcause` (0x342), `mtval` (0x343),
  `mip` (0x344), `mvendorid` (0xF11), `marchid` (0xF12), `mimpid` (0xF13),
  `mhartid` (0xF14), `mconfigptr` (0xF15). Any other CSR is written by
  number, and the assembler does not ask whether the core implements it.
- `uimm` is the 5-bit value in the place of `rs1`.

Directives:

| Directive | Effect |
|---|---|
| `.word VALUE` | Places one word, 0 to 4294967295. A negative value is refused. |
| `.org OFFSET` | Moves the location forward to byte `OFFSET` of the image and fills the gap with zero words. `OFFSET` is a multiple of 4, not below the present location, and at most 65536. `.org` to the present location does nothing. |

The location counter is the byte offset of the next word. It starts at 0
and is always a multiple of 4. A label takes the value the counter has
where the label stands. The assembler knows no load address: a branch to
a label holds the difference of two offsets, and code that needs an
absolute address builds it at run time with `auipc`, as the timer program
does.

An image is at most 16384 words, 65536 bytes.

## Errors

The assembler reads the whole file and reports every line that is wrong:
the line number, counted from 1, and the reason. A file with any error
gives no word and exit status 1. No value is ever cut to fit a field.

| Reason printed | When |
|---|---|
| `unknown mnemonic` | The first word of a statement is no mnemonic of the table above. |
| `bad register, expected x0 to x31` | An operand that must be a register is not one. |
| `immediate out of range for its field` | A number operand is outside the range of the table, or a `.word` value is. |
| `branch or jump offset is odd` | A numeric target is odd. This is judged before the range. |
| `branch or jump target out of range` | The offset to a label, or a numeric offset, is outside the range. |
| `undefined label` | A target names a label the file does not define. |
| `duplicate label` | A label is defined a second time. The error is on the second line. |
| `wrong operand count` | The number of operands is not that of the form. |
| `trailing text after an operand` | More text follows a complete operand, as in `add x1, x2, x3 x4`. |
| `bad number` | A number has no digit, a digit that is not of its radix, or does not fit 64 bits. |
| `expected imm(rs1)` | A load, a store or `jalr` has an operand that is not a number and a register in brackets. The number may not be left out. |
| `unknown CSR name` | A CSR operand is neither a number nor one of the 14 names. |
| `.org offset is not a multiple of 4 or lies before the location` | |
| `unknown directive` | A statement starts with a dot and is neither `.word` nor `.org`. |
| `image larger than 65536 bytes` | A statement or `.org` would pass the limit. |
| `bad label name` | A label definition or a target is not a label as the grammar has it. |
| `empty operand` | Nothing stands between two commas, or after the last one. |
| `more than 1024 labels` | |

One statement gives at most one error, the first found. A line with a bad
label definition and a bad statement gives two.

## Commands

| Command | What it does | Exit status |
|---|---|---|
| `asm-rv64 asm FILE` | Prints the words of the image in hex, one to a line, first word first. | 0, or 1 with errors |
| `asm-rv64 bytes FILE` | Prints the image as hex bytes, 16 to a line, lowest address first. | 0, or 1 |
| `asm-rv64 dis FILE` | Reads a file of hex words, one word of 8 hex digits to a line, and prints the text of each. | 0, or 1 when a line is no word |
| `asm-rv64 match FILE NAME` | Assembles `FILE` and compares the bytes with a built-in image of `emu-rv64`; `NAME` is `timer` or `trap`. | 0 when equal, else 1 |
| `asm-rv64 same FILE IMAGE` | Assembles `FILE` and compares the bytes with the bytes of the file `IMAGE`. | 0 when equal, else 1 |
| `asm-rv64 hand FILE GOLDEN` | Assembles `FILE` and compares the words, in order, with the 8 hex digits after every `word=` in `GOLDEN`, a golden file of the hand-computed cases of `emu-rv64`. | 0 when equal, else 1 |
| `asm-rv64 acases` | Runs the hand cases. | 0 when all pass |
| `asm-rv64 aprops SEED STEPS` | Runs the property rules. | 0 when accepted |
| `asm-rv64 acontrols` | Runs both checks against every planted defect. | 0 when every defect is refused |

A file that cannot be read gives exit status 3, a file longer than 65536
bytes status 5, and a command line that is no command status 2. Errors of
an assembly are printed as `FILE:LINE: error: REASON`, then a line with
their count.

The package does not write an image file. The standard library function
`write_bytes` takes its data as a string, and a string cannot hold a zero
byte; `emu-rv64` 0.4.0 found the same. A host tool turns the hex text into
bytes:

    asm-rv64 bytes tests/timer.s | xxd -r -p > timer.img

`asm` prints each word with its most significant digit first, and the
image stores the least significant byte first. `bytes` prints the order
of the file.

### Text of a word

`dis` prints, for each word, the instruction the decoder of `emu-rv64`
reads in it, or `.word 0x........` when the decoder refuses the word. The
text is in the operand forms above, with decimal numbers, hex for `imm20`,
`bits`, CSR numbers and `.word`, CSR names where the core has one, and a
branch or jump target as a numeric offset. It has no labels and no
comments. A data word that happens to be an instruction word prints as
that instruction: `0xDEADBEEF` prints as `jal x29, -150038`. Assembling the
printed text gives the words back.

## How it is checked

No other assembler, emulator or test suite was used, as a reference or in
any other way. The evidence is the encoder and decoder of `emu-rv64`, the
words its author assembled by hand, and words worked out from the field
layouts outside this package.

| Check | Command | What it does |
|---|---|---|
| Hand cases | `acases` | 359 cases. 231 are one statement that must give one given word: every mnemonic, each number operand at its lowest and highest value, at -1 and at 0 where the range has them, every CSR name, and ways of writing (spaces, signs, radix, a label or a comment on the line). 111 are one statement that must be refused with one given reason. 17 are programs: the four programs of `emu-rv64`, labels around `.word` and `.org`, comments and line endings, files with several errors, and the limits of 16384 words and 1024 labels. |
| Hand cases of the emulator | `hand` | `tests/hand-cases.s`, `tests/hand-mcases.s` and `tests/hand-icases.s` hold, as text, the 61, 59 and 50 instruction words that the commands `cases`, `mcases` and `icases` of `emu-rv64` 0.4.0 print. `hand` assembles each file and compares the words with the golden file of that command. `tests/emu-golden-*.txt` are copies of those golden files. |
| Programs of the emulator | `match`, `same` | `tests/timer.s` and `tests/trap.s` were written from the listings in the comments of `src/iprog.bld` and `src/mprog.bld` of `emu-rv64`. `match` compares the assembled bytes with the built-in image, which those files build from their hand-assembled words. `same` compares with `tests/emu-timer.img`, a copy of `tests/timer.img` of `emu-rv64`. |
| Properties | `aprops SEED STEPS` | Nine rules on seeded random input, listed below. |
| Controls | `acontrols` | The assembler with one planted defect at a time, ten in all. The hand cases must refuse each, and the property rules, run without the cases, must refuse each. |
| Text round trip | `dis`, `asm` | `tests/all.s` holds every mnemonic. `tests/cases.txt` assembles it, prints the text of the words, and assembles that text to the same words. |

Where an expected word comes from:

- The programs: the built-in images and the word lists of `emu-rv64`.
- The three hand-case files: the golden files of `emu-rv64`.
- The 231 single statements and the label programs: the sum of the shifted
  fields of the instruction format, computed outside the package by a
  short script written for this purpose. It shares no code with the
  encoder or the assembler. Before use it was run on the 170 hand-case
  words of `emu-rv64`: it decoded each and encoded the text back to the
  same word.

### Property rules

`STEPS` is a number of rounds. Each round judges rules 61 to 65 once.
Every eighth round makes one random program and judges rules 66 to 69 on
it. Random instructions come from the generator of `emu-rv64` for RV64I,
and from a generator of this package for Zicsr, MRET and WFI. The renderer
writes them in one of 16 styles: decimal or hex numbers, CSR names or
numbers, single spaces or tabs and extra spaces, with or without a plus
sign on an offset.

| Kind | Rule |
|---|---|
| 61 | For a random instruction, written in a random style, the assembler gives the word the encoder gives. |
| 62 | For a random 32-bit word the decoder accepts, and for an instruction word with one bit flipped that it accepts, the text of the decoded record assembles to the same word. |
| 63 | The canonical text of a random instruction assembles to the encoder's word, and the text of that word is the same text, byte for byte. |
| 64 | The lowest and the highest value of a number operand are accepted and give the encoder's word. |
| 65 | A value one past either end of a range, a value far outside it, and an odd branch or jump offset are refused: one error on line 1 with the right reason, and no word. |
| 66 | A random program gives the words the generator computed. The generator lays the program out before any text exists: 4 to 24 statements, 1 to 6 labels, branches and jumps to those labels, `.word` and `.org`. It counts 4 bytes per instruction and per `.word` and the size of each `.org` gap, so it has every offset without the assembler. |
| 67 | That program with the definition of a used label left out gives no word, only `undefined label` errors, the first on the first line that uses the label. |
| 68 | That program with one label defined once more on a last line gives no word and one error, `duplicate label`, on that line. |
| 69 | That program with a comment after every statement, comments after labels, blank lines and comment lines gives the same words. Every comment holds the text of an instruction. |

### Controls

A number passed down to the assembler selects the defect. Number 0 is the
assembler the commands use, and there is no second copy of its code.
`tests/controls.txt` names the function that holds each defect.

| Control | Planted defect |
|---|---|
| 1 | The offset to a label is measured from the next instruction. |
| 2 | Bits 4 to 0 and bits 9 to 5 of a store offset change places. |
| 3 | The minus sign of a number is dropped. |
| 4 | Every range check accepts one value past the top of the range. |
| 5 | Pass 1 does not count `.word`, so every later label is 4 bytes short. |
| 6 | A store takes `rs1` and `rs2` in each other's place. |
| 7 | The digits after `0x` are read in base 10. |
| 8 | The text after `#` is assembled as one more line. |
| 9 | A second definition of a label is accepted. |
| 10 | An undefined label reads as offset 0 of the image. |

## Limits

- One author wrote the assembler, the renderer, the hand cases, the
  property rules and the controls, and the same author wrote `emu-rv64`.
- No comparison against any other assembler was run. No file assembled
  here was assembled by another tool, and no architecture test was run.
- The encoder is the single source of the bit layouts. An error in it
  would be in the assembler too. What stands against that is the hand
  cases of `emu-rv64` and the script described above, both by the same
  author.
- The property rules write their text with the renderer of this package.
  A misreading shared by the renderer and the assembler, such as a wrong
  operand order in both, cancels in rules 61 to 64. The hand cases are the
  guard: their text was typed, and their words come from elsewhere.
- The three hand-case files were drafted by the script from the golden
  words and then read against the comments in the emulator source, which
  name the instruction of many cases and of every word of the timer and
  trap programs.
- No pseudo-instructions, no names of the calling convention, no
  expressions, no symbols other than labels, no relocation and no load
  address. `%hi` and `%lo` do not exist.
- `fence` is written in a form of this package. The usual form is refused.
- Mnemonics outside the table are unknown: no multiply and divide, atomic,
  floating-point or compressed instructions, no `sret`, no `fence.i`.
- The assembler encodes a Zicsr instruction for any CSR number. Whether
  the core implements the CSR, or lets it be written, is decided when the
  instruction runs.
- A source file is at most 65536 bytes and an image at most 65536 bytes.
- The package cannot write an image file, as "Commands" says.
- Field layouts were not read from the manual for this version. The manual
  keeps them in figure files, and only the chapter text was fetched. The
  text confirms what "Sources" lists.
- From memory and not confirmed against the manual text: the mnemonics
  themselves as spelled here; the operand form `imm(rs1)` for loads and
  stores (the text shows it for `jalr`); the order `rs2, imm(rs1)` of a
  store; that a branch or jump operand given as a number is an offset from
  the instruction; the 14 CSR names and their numbers, which are those
  `emu-rv64` took from the privileged manual for 0.2.0.
- Built and run on one Windows x86-64 host with one C compiler.

## Verified

Every line below was run on the 0.1.0 build, on one Windows x86-64 host.
The package was built with two builds of the compiler, and the 28
commands of `tests/cases.txt` printed the same bytes with both. The
package uses no compiler change that the main branch lacks.

- `acases`: 359 of 359 cases pass.
- `hand tests/hand-cases.s tests/emu-golden-cases.txt`: 61 words, equal.
  `hand` on the `mcases` files: 59 words, equal. On the `icases` files: 50
  words, equal. The three copies of the golden files are byte for byte
  those of `emu-rv64` 0.4.0.
- `asm tests/timer.s`: 22 words. `match tests/timer.s timer`: 88 bytes,
  equal to the built-in image. `same tests/timer.s tests/emu-timer.img`:
  equal; that file has the SHA-256
  `f908cafcb509b05f81d785db80c34a7cb374b3e817100f6bc72bde476655b9b6` and is
  byte for byte `tests/timer.img` of `emu-rv64` 0.4.0. The output of
  `bytes tests/timer.s` is byte for byte the output of
  `emu-rv64 image timer`, and passed through `xxd -r -p` it has the same
  SHA-256.

  The words of `tests/timer.s`:

      020042b7 06400313 0062b023 100006b7 02e00713 00000397 02c38393 30539073
      08000413 30441073 30046073 00150513 ffdff06f 00000000 00000000 00000000
      0002b583 06458593 00b2b023 00e68023 00160613 30200073

- `match tests/trap.s trap`: 84 bytes, equal to the built-in image.
- `asm tests/serial.s` and `asm tests/sum.s` print the 8 and the 6 words
  that `src/main.bld` of `emu-rv64` holds.
- `asm tests/all.s`: 77 words. `dis` of them and `asm` of that text: the
  same 77 words.
- `asm tests/errors.s`: 20 errors on 20 lines, each with its reason, and
  no word.
- A file of 65538 bytes is refused with exit status 5, and one of 65536
  bytes is assembled.
- `aprops`: accepted at every seed.

  | Seed | Rounds | Raw words | Decoded | Edge values | Refusals | Programs | Statements | Label left out |
  |---|---|---|---|---|---|---|---|---|
  | 1 | 20000 | 40000 | 15162 | 6620 | 13380 | 2500 | 34771 | 2332 |
  | 7 | 50000 | 100000 | 37720 | 16757 | 33243 | 6250 | 87483 | 5892 |
  | 4 | 200000 | 400000 | 150809 | 67128 | 132872 | 25000 | 350163 | 23506 |
  | 5 | 200000 | 400000 | 151051 | 66613 | 133387 | 25000 | 349933 | 23483 |
  | 9 | 200000 | 400000 | 150904 | 66763 | 133237 | 25000 | 350552 | 23475 |
  | 2 | 1000000 | 2000000 | 755245 | 333775 | 666225 | 125000 | 1751837 | 117450 |
  | 3 | 2000000 | 4000000 | 1507677 | 665653 | 1334347 | 250000 | 3501502 | 235023 |
  | 11 | 5000000 | 10000000 | 3771407 | 1666616 | 3333384 | 625000 | 8757656 | 587645 |

  "Raw words" counts the words tried under rule 62 and "Decoded" those the
  decoder accepted. "Label left out" counts the programs judged under rule
  67; a program in which no statement uses a label is not judged under it.

- `acontrols` (seed 1, 20000 rounds): the assembler accepted, 10 of 10
  planted defects refused by the hand cases and by the property rules.

  | Control | Hand cases that fail | First | Property rule |
  |---|---|---|---|
  | 1 | 9 | 139, `self: beq x1, x2, self` | 66 at round 0 |
  | 2 | 5 | 106, `sd x2, 8(x1)` | 61 at round 0 |
  | 3 | 60 | 20, `addi x3, x1, -2048` | 61 at round 1 |
  | 4 | 15 | 246, `addi x1, x2, 2048` | 65 at round 0 |
  | 5 | 1 | 347, the program with labels around `.word` | 66 at round 8 |
  | 6 | 24 | 90, `sb x2, -2048(x1)` | 61 at round 0 |
  | 7 | 35 | 107, `sw x5, 0x155(x6)` | 65 at round 0 |
  | 8 | 7 | 215, `addi x1, x2, 3 # note` | 69 at round 0 |
  | 9 | 2 | 352, the file with three errors | 68 at round 0 |
  | 10 | 4 | 280, `beq x1, x2, nowhere` | 67 at round 0 |

- `buildc pkg check-registry` accepts the registry with this package in
  it, and the registry package test of the compiler builds the package and
  compares the 28 commands of `tests/cases.txt` with their goldens.

Not verified: the package was not built or run on a second host or with a
second C compiler. Control 5 fails one hand case only, so that defect
rests on one case and on rule 66.

## Sources

- The RISC-V Instruction Set Manual, Volume I: Unprivileged Architecture,
  RISC-V International, CC BY 4.0, chapter sources from the
  `riscv/riscv-isa-manual` repository at commit
  `d50ad50f7dfad97a47a70ae0fab6a11660362167`:

  | File | SHA-256 |
  |---|---|
  | `src/unpriv/rv32.adoc` | `3f57167e4586f228f2c26df3e51aac5feab828ce8959bf0c745306f1f27df676` |
  | `src/unpriv/rv64.adoc` | `96896da94e87aef454a521d1a27f5ca6db0329500d729cf8bb4c169d0548c92e` |
  | `src/unpriv/zicsr.adoc` | `fd0adb3e18751be1e1fd06ad6f12039d7c6ff74e98ec0fc76a240e9d1a755b2b` |

  The text of these files states, and the assembler agrees:
  - the operand order `rd, rs1, imm` (`addi rd, rs1, 0`, `sltiu rd, rs1, 1`,
    `xori rd, rs1, -1`) and `rd, rs1, rs2` (`add t0, t1, t2`);
  - a branch written `bne t3, t4, overflow`: two registers, then a label;
  - the 12-bit immediate of `addi`, `slti`, `sltiu`, `andi`, `ori` and
    `xori` is sign-extended, also for `sltiu`;
  - a shift by a constant takes 6 bits in RV64I, and the 32-bit forms take
    5, with bit 5 set reserved;
  - the written form of `lui` and `auipc` leaves out the lower 12 bits of
    the value;
  - `lui ra, imm20` and `jalr ra, imm12(ra)` as written forms;
  - a branch offset is signed, in multiples of 2 bytes, within 4 KiB
    either way, and a `jal` offset within 1 MiB either way;
  - loads and stores add a sign-extended 12-bit offset to `rs1`, and a
    store takes its value from `rs2`;
  - the CSR number is a 12-bit field, the immediate forms take a 5-bit
    value that is not sign-extended, and the operand order is
    `rd, csr, rs1` (`csrrs rd, csr, x0`, `csrrw x0, csr, rs1`);
  - `#` starts a comment and `0x1f` is a hex number, in the examples.
- `emu-rv64` 0.4.0 of this registry: the encoder, the decoder, the
  operation numbers, the hand-assembled words and the two built-in images.
- The host tool `xxd` turned hex text into bytes for one comparison. It
  ran no program of the machine.
