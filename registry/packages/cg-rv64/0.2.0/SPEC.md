# cg-rv64 0.2.0: RV64IM code generator for BuildLang programs

A code generator is the part of a compiler that writes machine
instructions. This package is one for RISC-V, an open instruction set, in
its 64-bit form with the M extension (multiply and divide), written
RV64IM. It lets a BuildLang program run on the emulator `emu-rv64`.

Its input is MIR. The compiler `buildc` lowers a program
to MIR, its mid-level intermediate representation: functions made of
basic blocks, each a list of simple statements and one terminator (a
jump, a branch, a call or a return). `buildc mir emit FILE.bld -o
OUT.json` writes the MIR of a whole program as JSON with the schema name
`buildlang.mir/v1`. This package reads that file and prints assembly text
in the dialect of the assembler `asm-rv64`.

    buildc mir emit prog.bld -o prog.mir.json
    cg-rv64 gen prog.mir.json run 3 > prog.s          # entry function run(3)
    asm-rv64 serial prog.s 100000000                  # assemble and run
    cg-rv64 run prog.mir.json run 100000000 3         # the same in one step

Exactness class: `exact-int`. The same MIR file, entry name and arguments
give the same text on every host. The package reads no clock and draws no
random number.

The package is written in BuildLang. It depends on `asm-rv64` 0.4.0, which
depends on `emu-rv64` 0.7.0, so the command `run` assembles and runs in
one process.

## Changes from 0.1.0

0.1.0 is unchanged in the registry. 0.2.0 adds what a kernel needs to keep
its tables in globals and to build structs that hold arrays:

- **Struct literals with an array field** beside other fields. 0.1.0
  refused them, because the hosted program built a wrong value (G26). The
  hosted backend builds them correctly since buildlang PR 254, so the
  generator compiles them, with the same traps as the host.
- **Mutable globals and aggregate globals.** Every global of a covered type
  has a place in a global area at the bottom of the heap, filled with its
  initial value by the start code. Reads (`Use`, `FieldAccess`,
  `IndexAccess` on a global), stores (`GlobalStore`) and the address of a
  global (`AddressOfGlobal`, new in the MIR with the compiler fix for G35)
  are covered, with the bounds check on every index.
- **The place shapes of PR 254 and the G35 fix**: an index through a
  reference to an array or a slice inside a place (`xs[i].w[j] = v` with
  `xs: &mut [S]`), a slice made from a pointer to a whole array (`f(&mut
  s.w)`, `f(&mut GLOBAL)`), and an indexed store or read through a
  reference to an array.
- Six planted defects, 23 to 28, for the new paths.

Two MIR forms that 0.2.0 reads are written only by `buildc` with the
compiler fix for G33 to G35: `AddressOfGlobal` and the array constant of a
global's initial value. MIR from an older `buildc` holds neither. An
immutable integer global with an integer initial value is still folded
into the code where it is read and has no place, so a program with no
other global gets the same assembly text from 0.2.0 as from 0.1.0.

## Commands

| Command | What it does | Exit status |
|---|---|---|
| `cg-rv64 gen MIR ENTRY [ARG...]` | Prints the assembly text of the program that starts in function `ENTRY`. | 0; 1 after a refusal |
| `cg-rv64 run MIR ENTRY MAX [ARG...]` | Generates, assembles with the large limits of `asm-rv64`, runs the image on the serial run of `emu-rv64` for at most `MAX` steps and prints the bytes the program sent to the serial device, and nothing else. A summary line goes to standard error. | The exit code of the program, cut to 0 to 255; 124 when it did not halt; 125 when a byte could not be printed; 1 after a refusal |
| `cg-rv64 run-check MIR ENTRY MAX [ARG...]` | The same run twice in one memory. Prints the image size, whether the program halted, its exit code, the step count, the number and FNV-1a hash of the serial bytes, the state hash, and whether the second run agreed. | 0 when the program halted and the runs agree |
| `cg-rv64 genv V MIR ENTRY [ARG...]`, `cg-rv64 runv V MIR ENTRY MAX [ARG...]` | `gen` and `run` with planted defect `V`, for the checks. `V` = 0 is the generator. | as `gen`, `run` |
| `cg-rv64 controls` | Prints the list of planted defects. | 0 |

`ENTRY` is the name of a function of the program. Each `ARG` is a decimal
integer, with a minus sign when negative, for one parameter of that
function, in order. The entry function takes at most eight parameters,
each an integer or a Bool (0 or 1), and returns nothing or a scalar, which
is dropped. An argument outside the range of its parameter type is
refused.

Only the functions the entry function can reach through direct calls are
compiled. A hosted `main` that reads the command line is therefore not
compiled when the entry is a function it calls.

A refusal prints one line, `refused: function NAME: CONSTRUCT`, and no
assembly text. A missing file gives `error: cannot read FILE` and status
3, an argument that is not a number status 2, and a command line that is
no command status 2.

## What is covered

Types: `bool`, the integers `i8` to `i64`, `u8` to `u64`, `isize` and
`usize` (64 bits), references and raw pointers, structs, fixed arrays,
slices, and vectors (`Vec`) of integers.

| MIR construct | Covered form |
|---|---|
| `Assign` | To a local of any covered type. An assignment to or from a `Void` local does nothing. |
| `Use` | A local, an integer or Bool constant, an immutable integer global, a string constant stored to a pointer local. Since 0.2.0: a global with a place, scalar or aggregate. |
| `BinaryOp` | `Add Sub Mul Div Rem`, `AddWrapping SubWrapping MulWrapping`, `BitAnd BitOr BitXor`, `Shl Shr`, `Eq Ne Lt Le Gt Ge`. |
| `UnaryOp` | `Not` on a Bool, `BitNot` and `Neg` on an integer. |
| `Cast` | `IntToInt`, from an integer or a Bool to an integer. |
| `Ref`, `AddressOf` | The address of a local, or of a part of it named by `Field`, `Index` and `Deref` projections. Since 0.2.0 an `Index` may step into a slice, or through a reference to an array or a slice. |
| `AddressOfGlobal` | Since 0.2.0: the address of a global with a place. |
| `FieldAccess` | A field of a struct local or of a struct behind a pointer; `len` of a slice. Since 0.2.0: a field of a global struct. |
| `IndexAccess` | An element of an array local, a slice local, a slice behind a pointer, or a vector. Since 0.2.0: an element of an array behind a pointer and of an array global. |
| `Deref` | A read through a pointer local. |
| `Aggregate` | An array literal, a struct literal (since 0.2.0 also one with an array field beside other fields), and a slice made from a whole array local or, since 0.2.0, from a pointer local to a whole array. |
| `FieldAssign`, `FieldDerefAssign`, `DerefAssign`, `IndexStore` | Stores of a scalar, or of an aggregate copied from a local. |
| `GlobalStore` | Since 0.2.0: a store into a global with a place, of a scalar or of an aggregate copied from a local. |
| `StorageLive`, `StorageDead`, `Nop` | Nothing is emitted. |
| `Goto`, `If`, `Switch`, `Return` | `If` on a Bool; `Switch` on an integer local with integer targets. |
| `Call` | A direct call of a function with a body in the MIR; `printf`; `Vec_new`, `build_hvec_len`, `build_hvec_push_T`, `build_hvec_set_T`. |

Everything else is refused: floating point, 128-bit integers, strings
other than constants (`BuildString`), tuples, options, enums, maps,
closures, trait objects, function pointers, variadic functions, `Assert`,
`Abort`, `Unreachable`, `Drop`, saturating and checked-pair arithmetic,
casts other than integer casts, and any call of a runtime function not
named above (`exit`, `args_get` and file access among them). A global
gets no place when its type is not covered, holds a vector, or has an
initial value other than an integer, Bool, struct, array or zero
constant; any use of such a global is refused by name.

A block that cannot be reached from the first block of its function is
not compiled, so a construct in it is not refused. The MIR holds such
blocks after a `return`.

## Arithmetic and its traps

The MIR does not carry the checks of the language. The hosted C backend
(`compiler/src/codegen/backend/c.rs`) adds them, and the C runtime it
emits (`compiler/src/codegen/runtime.rs`) holds them. Each rule below was
read from those two files and restated for RISC-V. A trap is the end of
the program: it prints one line and stops.

A register holds a scalar in the form of its type: the low bits of the
type, extended to 64 bits by its sign. A Bool is 0 or 1. A local lives in
memory at the width of its type and is loaded with the sign of its type.

| Operation | Rule | Trap line |
|---|---|---|
| `Add`, `Sub`, `Mul` | Exact result in the type of the operation, or a trap. For a width below 64 the result is computed in 64 bits and compared with itself cut to the width. At 64 bits: signed add and subtract compare the sign of the right operand with the direction the result moved; unsigned add and subtract compare with the left operand; multiply compares the high half of the product (`mulh`, `mulhu`) with the sign of the low half, or with zero. | `attempt to add with overflow`, `attempt to subtract with overflow`, `attempt to multiply with overflow` |
| `Div`, `Rem` | A zero divisor traps. For a signed type the smallest value divided by -1 traps. Otherwise `div`, `divu`, `rem`, `remu`. | `attempt to divide by zero`, `attempt to calculate the remainder with a divisor of zero`, `attempt to divide with overflow`, `attempt to calculate the remainder with overflow` |
| `AddWrapping`, `SubWrapping`, `MulWrapping` | The result cut to the width. | none |
| `Neg` | Signed: the smallest value traps. Unsigned: wraps. | `attempt to negate with overflow` |
| `Shl`, `Shr` | The count is masked to the width of the left operand, minus one. `Shl` cuts the result to the width. `Shr` copies the sign for a signed type and fills with zeros for an unsigned one. | none |
| `BitAnd`, `BitOr`, `BitXor`, `BitNot` | Bit by bit in the type. | none |
| `Eq` to `Ge` | By the sign of the type. | none |
| `IntToInt` | Cut to the width of the target, extended by the sign of the target. | none |
| Index into an array, a slice or a vector | The index, read as an unsigned 64-bit number, must be below the length. | `index out of bounds: the len is N but the index is I`, with `I` printed signed; a store to a vector prints `Vec index out of bounds: I >= N` |

The type of an arithmetic operation is chosen as the C backend chooses the
helper function: the type of the left operand when it is a local, else of
the right when it is a local, else of a global, else the type written on
a constant. Both operands are converted to that type, as C converts the
arguments of the helper.

A local of one integer type used where another integer type is expected
(an assignment, an argument, a return, an operand of arithmetic) is
converted as C converts it: cut to the width and extended by the sign of
the target. The compiler needs this: it gives the result of a `match` the
type of the matched value.

Comparisons and the bit operations are C operators in the hosted program,
and C compares two operands of different types after its own promotion
rules. The generator covers the cases where those rules give the plain
answer: both operands of one type, or one of them a constant in the range
of the other's type and of the same signedness class (an unsigned
constant against a signed local is refused). Other mixes are refused.

## printf

`println!` reaches the MIR as a call of `printf` with a C format string.
The generator reads the format when it compiles the call. It emits one
runtime call for each run of plain bytes and one for each conversion, so
the runtime holds no format interpreter, and a conversion outside the
list is refused when the program is generated.

| Conversion | Argument | Output |
|---|---|---|
| `%d`, `%u` | A signed or an unsigned integer of at most 32 bits | Decimal |
| `%lld`, `%llu` | A signed or an unsigned integer of 64 bits | Decimal |
| `%x`, `%llx`, with an optional `0` and a width up to 64 | An integer of at most 32 bits, or of 64 bits with `ll` | Lower-case hex. Without `ll` the low 32 bits are printed, as C prints an `int`. Padding is zeros with `0`, else spaces. |
| `%s` | A Bool, or a string constant | `true` or `false`; the bytes of the string |
| `%%` | none | `%` |

The format must be one string constant: the first argument is a string
constant, or a local that the function assigns exactly once, from a
string constant. The number of arguments must equal the number of
conversions. Strings may hold the escapes `\n`, `\t`, `\r`, `\"`, `\\` and
`\/`; a `\u` escape is refused.

## Machine contract

The program is a flat image: raw bytes with no header.

| Item | Value |
|---|---|
| Load address | `0x80000000`. The image is at most 1048576 bytes. |
| Entry | Offset 0, in machine mode, as `emu-rv64` resets its trap profile. |
| Memory | 4194304 bytes, `0x80000000` to `0x803FFFFF`. |
| Stack | Starts at `0x80400000` and grows down. |
| Globals | Since 0.2.0: the global area starts at `0x80100000`. Each global with a place starts at a multiple of 8 and is laid out as a local is; the area is rounded up to 16 bytes. The start code fills it with zeros and then stores every initial value the MIR gives. |
| Heap | Starts after the global area (at `0x80100000` when there is none) and grows up. Memory is never given back. |
| Serial device | One byte stored to `0x10000000` is one byte of output. |
| Halt | The program jumps to the word `0x0000006F`, `jal x0, 0`, with its exit code in `x10`. The serial run of `emu-rv64` stops there. |
| Exit codes | 0: the entry function returned. 101: a trap of the table above. 102: a machine trap. 103: stack overflow. 104: out of memory. |

Offset 0 holds a jump to the start code, and offset 4 a jump to the
machine trap handler; the start code writes the address of offset 4 to
`mtvec`. The start code then sets the stack pointer, the heap pointer and
the stack limit, fills the global area (since 0.2.0; the heap pointer and
the stack limit move past it), loads the entry arguments, calls the entry
function and halts with code 0.

Registers: `x1` return address, `x2` stack pointer, `x10` to `x17`
arguments and `x10` the result, `x26` the next free heap address, `x27`
the stack limit (1024 bytes above the heap). `x5` to `x7` and `x28` to
`x31` are scratch. No value lives in a register across two statements:
every local has a slot in the frame of its function.

### Calling convention

The convention is this package's own. It is not the RISC-V psABI, the
standard calling convention, which was not read for this version.

- Arguments are numbered from 0. When the result is an aggregate (a
  struct, an array or a slice), argument 0 is hidden: the address the
  result is written to.
- Arguments 0 to 7 travel in `x10` to `x17`. Argument `k` from 8 on is
  stored by the caller at `8 * (k - 8)` bytes above its stack pointer.
- A scalar travels as its value in the form of its type. An aggregate
  travels as the address of the caller's copy, and the callee copies it
  into its own frame before it runs.
- A scalar result comes back in `x10`.
- The callee may change every register except `x2`, `x26` and `x27`.

A frame holds, from the stack pointer up: the outgoing stack arguments,
the locals (each at a multiple of 8), the hidden result address, the
return address. Its size is a multiple of 16.

### Layout of data

Sizes in bytes: Bool 1, an integer its width, a pointer 8, a vector 8 (the
address of three doublewords: the address of the elements, the length,
the capacity), a slice 16 (address, then length). A struct holds its
fields in order, each at the next multiple of its own size or of the
largest scalar inside it, and is rounded up to the largest. No other
program reads the memory of a generated program, so this layout need not
be the layout of C.

A constant outside -2048 to 2047, and every string, is kept in the code:
a `jal` steps over the data words and leaves their address in a register.
The assembler knows no load address, and this form needs none.

## Differences from the hosted program

| Event | Hosted program | Generated program |
|---|---|---|
| Output | Standard output | The serial device |
| A trap of the table | One line on standard error, exit status 101 | The same line on the serial device, after the bytes already sent; halt with code 101 |
| An index in a projected store, `a[i].f = v` with `a` an array of structs | Checked since buildlang PR 254 (G28); before it, an index past the end wrote outside the array | Checked: the `index out of bounds` trap |
| A call chain deeper than the stack | The operating system ends the process | `stack overflow`, code 103, when a function entry would move the stack pointer below the limit |
| No memory left for a vector | `realloc` fails and the program crashes | `out of memory`, code 104 |
| A load or store outside memory | The operating system ends the process | `machine trap: mcause=.. mepc=.. mtval=..`, code 102 |
| Entry | `main`, with the command line | Any function, with integer arguments fixed when the program is generated |
| A vector whose elements have another width than its type says | Refused at run time by the C runtime | Cannot be written in the covered set; not checked |

The rule by which a hosted run and an emulated run are judged equal: the
serial bytes equal the hosted standard output followed by the hosted
standard error, and the halt code equals the hosted exit status. The
hosted runtime writes both streams unbuffered, so one pipe holds them in
the order they were written.

## How it is checked

No other compiler backend for RISC-V, no other assembler, no other
emulator and no test suite was used. The reference is the hosted build of
the same program: `buildc` with its C backend and the host C compiler.

| Check | Where | What it does |
|---|---|---|
| Slice target | `tests/tools/kernels.sh`, `tests/cases.txt` | The function `trace` of `os-pfa` 0.1.0 and of `os-runq` 0.1.0, compiled from the unchanged registry sources, prints on the emulator the bytes of each committed golden trace, for the core and for the reference model. |
| Differential corpus | `tests/tools/diff.sh`, `tests/corpus.txt`, `tests/programs/` | 16 programs, 172 runs, each on the host and on the emulator, compared by the rule above. The three programs added in 0.2.0 are written by `tests/tools/gen_corpus2.py`. |
| Differential fuzz | `tests/tools/fuzz.sh`, `tests/tools/fuzzgen.bld` | A seeded generator of programs, written in BuildLang; each program runs with three arguments on both sides. |
| Controls | `cg-rv64 controls`, `tests/controls.txt` | 28 planted wrong-code defects. Each must be refused by the corpus; 1 to 22 also by the fuzz, whose programs hold no global and no struct with an array field, so 23 to 28 are refused by the corpus alone. |
| Fixtures | `tests/cases.txt`, `tests/mir/` | Committed MIR files with the expected assembly text and serial bytes, so the registry test covers the package with no network and no second compiler run. |

### The kernel traces

`os-pfa` and `os-runq` are used as published. Their hosted `main` reads
the command line; the emulator has none. The arrangement is the smallest
found: the entry function is their own `trace(use_ref, SIZE, SEED,
STEPS)`, and the four values are entry arguments of `cg-rv64`. No file
of either package is edited or copied into this one, and there is no new
version of them.

    tests/tools/kernels.sh os-pfa 70 5 400
    tests/tools/kernels.sh os-runq goldens

The script compares the SHA-256 of every source file it builds with the
registry file, checks that the committed fixture is the MIR of those
sources, runs the hosted program and the emulator, and compares the
bytes. `trace` builds the reference model in both modes, a vector of one
byte per frame or slot, which is why vectors are in the covered set.

The hosted `main` refuses a SIZE outside the range of the package before
it calls `trace`. The entry function is `trace` itself, so that check
does not run on the emulator: outside the range the two sides are not
expected to agree, and the script does not judge such a run.

### The corpus

| Program | Content |
|---|---|
| `edge_i8` to `edge_u64`, eight programs | For nine edge values of the type (eight for an unsigned type), all pairs: wrapping add, subtract and multiply; the six comparisons; and, or, exclusive or, not; divide and remainder where they do not trap; checked add, subtract and multiply where they do not trap; negate; shifts by 0, 1, width - 1, width, width + 1 and two larger counts; casts to all eight types; three hex forms. |
| `traps_s`, `traps_u` | One run for each trap of each type: add, subtract and multiply past either end, a zero divisor for divide and for remainder, negate of the smallest value, the smallest value divided by -1 and its remainder; and runs just inside the range, which must not trap. |
| `structs` | Fields of mixed widths, nested structs, arrays of structs, arrays of arrays, struct arguments and results, writes through references. |
| `calls` | Recursion, mutual recursion, twelve arguments of mixed widths, an aggregate past the eighth argument, references to scalars, `match` on an integer, slices; six index traps: an array, a slice, a negative index, a vector read, a vector store, a large index. |
| `vecs` | Vectors of bytes and of 64-bit words past their first buffer, a vector behind a reference, slices of bytes, the forms of printf. |
| `sarrays` (0.2.0) | For each of the eight integer types: struct literals with a three-element array in the middle, a four-element array first and a one-element array last; a store into an array field by an index from the argument; a struct with an array field passed by value and changed by the callee; a nested struct and an array of such structs with stores into their array fields; an element store through `&mut [S]`. Runs 2 and 3 trap: an index past a slice of structs, an index past an array field. |
| `globals` (0.2.0) | For each of the eight integer types: an array global and a struct global with initial values (struct fields written out of declaration order), an array of structs in a global with a zero initial value; stores by an index from the argument, `^=`, a store through `&mut [T; 3]`, through `&mut [T]` made from a global's array field, a `&mut self` method on a global, a whole-array store, and every element printed after. Also a `u64` global counted past 2^32 and reads of an immutable global array. Runs 2 to 6 trap: a store past an array global, past a global's array field, past an array of structs in a global, and reads past an array global and past an immutable global array. |
| `byval` (0.2.0) | For each of the eight integer types: a fixed array passed by value and changed by the callee by element, by whole-array assignment and as a two-dimensional array; the caller's arrays printed after. |

The eight `edge_` programs and the two trap programs are written by
`tests/tools/gen_corpus.py` from one table of values per type.

### The fuzz

`fuzzgen SEED` prints a program with a struct of three integer fields, an
array of eight integers passed as a slice, a function that returns the
struct, and five functions of ten parameters each (two travel on the
stack). Bodies hold declarations, assignments, field and element stores,
`if`, counted loops, calls, and decimal and hex prints. Expressions use
every integer width with wrapping, checked, divide, bit, shift and cast
operations. Function `fI` calls only `fJ` with `J < I`, so every program
ends. A program may trap: one divisor in sixteen may be zero, one element
store in six uses an index range one too large, one signed divide in
eight is the smallest value divided by -1 or -2, and the vector read at
the end of `run` is past the end for one argument in four. The fuzz
compares a trap as it compares any other outcome.

A seed whose program the hosted compiler refuses, or the generator
refuses, is counted and skipped, and the counts are printed.

### Controls

The number of a planted defect is passed down to the generator. Number 0
is the generator the commands use, and there is no second copy of its
code. `tests/controls.txt` lists, for each defect, the first corpus run
and the first fuzz seed that refuse it.

| Control | Planted defect | Corpus: first run that differs | Fuzz: first comparison that differs |
|---|---|---|---|
| 1 | A signed value narrower than 64 bits is extended with zeros. | run 1, `structs` argument 0 (host status 0, emulator 101) | comparison 4, seed 2 |
| 2 | Unsigned values are compared as signed. | run 31, `edge_u64` argument 0 (host status 0, emulator 101) | comparison 1, seed 1 |
| 3 | An array or slice index is not checked against the length. | run 13, `calls` argument 8 (host status 101, emulator 0) | comparison 13, seed 5 |
| 4 | A field read through a pointer takes the offset of the field before it. | run 1, `structs` argument 0 (host status 0, emulator 0) | comparison 10, seed 4 |
| 5 | Arguments 1 and 2 travel in each other's register. | run 1, `structs` argument 0 (host status 0, emulator 102) | comparison 1, seed 1 |
| 6 | An add that leaves its type does not trap. | run 32, `traps_s` argument 0 (host status 101, emulator 0) | comparison 61, seed 22 |
| 7 | A zero divisor does not trap. | run 35, `traps_s` argument 3 (host status 101, emulator 0) | comparison 151, seed 53 |
| 8 | Every shift count is masked with 63. | run 24, `edge_i8` argument 0 (host status 0, emulator 0) | comparison 5, seed 2 |
| 9 | A cast of a signed value to a wider type fills with zeros. | run 1, `structs` argument 0 (host status 0, emulator 0) | comparison 1, seed 1 |
| 10 | A stack argument is read one slot too high. | run 5, `calls` argument 0 (host status 0, emulator 102) | comparison 1, seed 1 |
| 11 | A subtract that leaves its type does not trap. | run 33, `traps_s` argument 1 (host status 101, emulator 0) | comparison 43, seed 15 |
| 12 | A multiply that leaves its type does not trap. | run 34, `traps_s` argument 2 (host status 101, emulator 0) | comparison 19, seed 7 |
| 13 | An indexed store writes eight bytes whatever the element width. | run 5, `calls` argument 0 (host status 0, emulator 0) | comparison 5, seed 2 |
| 14 | A signed remainder is taken as unsigned. | run 24, `edge_i8` argument 0 (host status 0, emulator 0) | comparison 4, seed 2 |
| 15 | An aggregate result is copied back for its first 8 bytes only. | run 1, `structs` argument 0 (host status 0, emulator 101) | comparison 1, seed 1 |
| 16 | A hex number is padded with spaces where the format asks for zeros. | run 19, `vecs` argument 0 (host status 0, emulator 0) | comparison 1, seed 1 |
| 17 | A negative decimal number is printed with no minus sign. | run 1, `structs` argument 0 (host status 0, emulator 0) | comparison 1, seed 1 |
| 18 | Less-or-equal is compiled as less-than. | run 24, `edge_i8` argument 0 (host status 0, emulator 0) | comparison 51, seed 18 |
| 19 | A vector index is not checked against the length. | run 16, `calls` argument 11 (host status 101, emulator 0) | comparison 3, seed 1 |
| 20 | A wrapping subtract takes its operands the wrong way round. | run 24, `edge_i8` argument 0 (host status 0, emulator 101) | comparison 1, seed 1 |
| 21 | The smallest signed value divided by -1 does not trap. | run 42, `traps_s` argument 10 (host status 101, emulator 0) | comparison 4, seed 2 |
| 22 | A right shift of a signed value fills with zeros. | run 27, `edge_i64` argument 0 (host status 0, emulator 0) | comparison 28, seed 10 |
| 23 | An array member of a struct literal is written one element past its field. | run 160, `sarrays` argument 0 (host status 0, emulator 0) | not reached |
| 24 | An array member of a struct literal is copied short by one element. | run 160, `sarrays` argument 0 (host status 0, emulator 0) | not reached |
| 25 | An index into an array reached through a global's address is not checked. | run 166, `globals` argument 2 (host status 101, emulator 0) | not reached |
| 26 | A store into a whole global writes eight bytes past the global. | run 164, `globals` argument 0 (host status 0, emulator 0) | not reached |
| 27 | The initial value of a global array element is written to the element after it. | run 164, `globals` argument 0 (host status 0, emulator 0) | not reached |
| 28 | An index into a slice inside a place reads the length from the address word. | run 162, `sarrays` argument 2 (host status 101, emulator 0) | not reached |

"Not reached": for seeds 1 to 80 the assembly that `genv V` writes for
every fuzz program equals that of the generator, so the fuzz cannot
refuse defects 23 to 28; the corpus refuses each.

## Limits

How each passing check could accept a wrong generator:

- **One reference.** Every expected byte comes from the hosted C backend.
  A rule that the C backend and this generator both get wrong passes
  every check. Four defects of the hosted side were found while building
  the corpus (see "Verified"); each was kept out of the corpus, so the
  corpus says nothing about those constructs.
- **Only printed values are compared.** A wrong value that is never
  printed, and never changes what is printed, is not seen. Memory outside
  the printed locals is not compared: a store that lands on a neighbour
  which is not read again passes.
- **Two planted defects were invisible and were replaced.** "An integer
  cast keeps the bits above the target width" and "a narrow wrapping
  operation keeps the bits above its width" changed no output, because
  every value passes through a typed memory slot that cuts it again. The
  redundancy is real, and it means a missing cut inside an expression
  would not be found by these checks if a later version kept values in
  registers.
- **The covered set is the tested set, with gaps.** `Switch` is covered by
  code and was not reached by any test program: the compiler lowers
  `match` on an integer to a chain of `If`. `Deref` projections, slices
  of 16-bit and 32-bit elements behind a pointer, vectors of 16-bit and
  32-bit elements, `isize`, and frames larger than 2047 bytes have little
  or no coverage.
- **The fuzz draws from one shape of program.** No recursion, no nested
  struct, no array of structs, no slice of a narrow type other than the
  one array. Its comparisons mix types only as the type checker allows.
  It holds no global and no struct with an array field, so the paths
  added in 0.2.0 are checked by the three corpus programs and the
  fixtures alone.
- **The global area is the generator's own layout.** Nothing compares it
  with the layout of the hosted C program; a program that reads a global
  through a pointer cast to another type is outside the covered set.
- **Two globals never share a place**, and the generator does not check
  that a pointer into the global area stays inside the global it was taken
  from: an index is checked against the array it indexes, as on the host,
  and nothing more.
- **Traps end a run.** A program that traps early tests little after the
  trap. About one fuzz run in three ends in a trap.
- **Refusal is tested thinly.** The fixtures hold a few refusals. That
  every construct outside the covered set is refused rests on the code:
  each MIR tag is matched by name and the last branch refuses.
- **One author.** The generator, the runtime, the corpus and the fuzz
  generator were written in one session by one author, and `emu-rv64`
  and `asm-rv64` under them have one author each as well. A shared
  misreading of an instruction would pass, unless the hosted result
  disagrees.
- **The instruction set was not re-read.** The generator uses 40
  mnemonics. What each does was taken from the specification of
  `emu-rv64` and from memory, and no chapter of the RISC-V manual was
  read for this version.
- **Determinism is shown on one host.** `run-check` runs twice in one
  process.
- **Not optimised.** Every local is loaded and stored at every use. The
  kernel traces take 0.1 to 7 million steps.
- **Mismatch with the C layout.** A program that depends on the layout of
  a struct in memory, through a pointer cast, is outside the covered set
  and refused; nothing checks layout against C.

## Verified

0.2.0 was checked on one Windows host with gcc 13.2.0 as the host C
compiler and `buildc` built from buildlang branch `compiler/g33-g35-g37`
at commit 7170ab59, the compiler that writes `AddressOfGlobal` and array
constants. Each line was run and its result read.

| Checked | By what | Result |
|---|---|---|
| The 7 committed golden traces of `os-pfa` 0.1.0 and `os-runq` 0.1.0, core and model | `tests/tools/kernels.sh PACKAGE goldens` | 14 runs, each equal three ways; both fixtures equal the MIR of the registry sources |
| Differential corpus | `tests/tools/diff.sh tests/corpus.txt tests/programs` | 172 comparisons, 0 different; the 13 new ones include 7 that end in a trap with status 101 on both sides |
| Differential fuzz | `tests/tools/fuzz.sh 1 80` | 80 programs, 231 comparisons, 0 different, 119 ended in a trap, 3 refused by the generator: the counts of 0.1.0 |
| Planted defects | `diff.sh -v V -x` for V = 1 to 28 | 28 of 28 refused by the corpus; 1 to 22 at the same first run as in 0.1.0 |
| Defects 1 to 22 and the fuzz | `genv V` of 0.1.0 and of 0.2.0 on the 80 fuzz programs and 7 corpus programs, V = 0 to 22 | 1840 and 161 assembly texts, every pair equal, so the fuzz results of 0.1.0 for these defects hold for 0.2.0 |
| Defects 23 to 28 and the fuzz | `genv V` against `genv 0` on the 80 fuzz programs | 480 pairs equal: the fuzz does not reach the new paths |
| Fixtures | every line of `tests/cases.txt` against the built program | 56 of 56 equal |
| The nine 0.1.0 MIR fixtures | regenerated with the compiler above | 9 of 9 byte for byte equal |
| 0.1.0 on the new programs | `cg-rv64` 0.1.0 `gen` on `sarrays` and `globals` | refused: the struct literal with an array field, and the operand `Global` |
| Lint | `buildc lint` on the joined source | 0 errors, 34 warnings, the same as 0.1.0, none in a file of this package |

Stated plainly, not verified:

- No program was run on RISC-V hardware or on another emulator.
- The RISC-V manual was not read for this version.
- The corpus and the fuzz ran on one host with one C compiler.
- The layout of the global area is not compared with anything; only the
  values a program prints are.

Defects of the hosted side found while writing the 0.1.0 corpus. Each was
kept out of that corpus. The first three were repaired in buildlang PR 254
(G26, G27, G28) and the fourth there as well (G29); the 0.2.0 corpus uses
the first three shapes.

| Defect | Seen as |
|---|---|
| A struct literal with an array field beside other fields | The hosted program prints other values than the literal holds: the C compound literal takes the address of the array as its first element. The generator refuses the literal. |
| `s.w[i] = v` with `w` an array field of a struct local | The MIR stores into a copy of the array, so the value is lost on the host and on the emulator alike. |
| `a[i].f = v` with `a` an array of structs | The hosted program does not check `i`. The generator checks it. |
| Two constants of one name in the joined sources of a package | The type checker accepts them and the C compiler refuses the program. |

## Sources

- `compiler/src/codegen/ir.rs`: the MIR data model and its JSON form.
- `compiler/src/codegen/backend/c.rs` and `compiler/src/codegen/runtime.rs`:
  the hosted lowering of every operation and the text of every trap.
- `SPEC.md` of `emu-rv64` 0.7.0 and of `asm-rv64` 0.4.0: the machine, the
  serial run and the assembly dialect.
