# Native x86-64 executables

`buildc` can write a runnable x86-64 executable by itself. It lowers the
program to machine code, adds a small runtime it also writes as machine code,
and wraps the result in the operating system's file format. No C compiler,
assembler or linker runs.

```
buildc main.bld --target x86_64-native -o main.exe
./main.exe
```

| Flag | Output |
|------|--------|
| `--target x86_64-native` | the host's format: PE32+ on Windows, ELF64 on Linux |
| `--target x86_64-windows` | Windows x64 console executable (PE32+), imports `GetStdHandle`, `WriteFile` and `ExitProcess` from kernel32 |
| `--target x86_64-linux` | Linux x86-64 static executable (ELF64), uses the `write` and `exit_group` system calls |

Functions follow the Microsoft x64 calling convention in PE files and System V
in ELF files. The program's exit code is the value `main` returns (0 for a
`main` with no return value).

This is a first slice. A program that uses anything outside it is refused at
compile time with a message naming the construct, and no file is written.

## What each backend compiles

"yes" for wasm means the construct is covered by the wasm backend's Node
tests. "not checked" means this page has no test for it either way.

| Construct | C backend | wasm | x86-64 native |
|-----------|-----------|------|---------------|
| i8 to i64, u8 to u64, isize, usize arithmetic (`+ - * / %`, bitwise, shifts) | yes | yes | yes |
| `wrapping_add`, `wrapping_sub`, `wrapping_mul` | yes | yes | yes |
| `checked_*`, `saturating_*`, `pow` | yes | not checked | refused |
| comparisons, `bool`, `&&`, `\|\|`, `!` | yes | yes | yes |
| locals and assignment | yes | yes | yes |
| `if` / `else`, `while`, `loop`, `break`, `continue` | yes | yes | yes |
| function calls and recursion | yes | yes | yes, up to 4 arguments on Windows and 6 on Linux |
| `println` / `print` of string literals, integers and bools | yes | yes | yes, plain `{}` holes |
| `println!` macro with width, flags, hex | yes | not checked | refused |
| `eprintln` (stderr) | yes | not checked | refused |
| i128, u128 | yes | not checked | refused |
| f32, f64 | yes | yes | refused |
| structs, tuples, enums | yes | structs yes | refused |
| arrays and indexing | yes | yes | refused |
| `Vec`, `String`, `format!` | yes | yes | refused |
| closures, traits, generics | yes | not checked | refused |
| calls into the runtime library or `extern` functions | yes | some | refused |

## How it is tested

`compiler/tests/native_exe.rs` builds nine corpus programs (fib, gcd, a
wrapping sum, nested calls, branches, u64 and i64 division and remainder,
negative numbers, a line mixing text and numbers, and `loop` with `break`
and `continue`) with the native target, runs each one, and requires its
stdout and exit code to equal the C backend's for the same source. A
planted-fault test flips one byte of the digit-printing routine in a built
executable and checks that the comparison then fails. A third test checks
that floats and structs are refused by name.

The Windows build runs on Windows and the ELF build runs on Linux; each host
runs only its own format.
