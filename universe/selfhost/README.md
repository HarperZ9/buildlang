# BuildLang Self-Hosting Compilers (Slices 3 through 10)

This directory contains pure BuildLang self-hosted compilers and zero-C kernel payloads that compile themselves across Stage 0, Stage 1, Stage 2, and Stage 3 to a cryptographic C99 fixed point (`sha256(stage2.c) == sha256(stage3.c)`) while emitting standalone executable binaries without an external C compiler or linker.

## Compiler Ladder

`buildc_selfhost_s3.bld` implements Slice 3: a lexer, recursive-descent parser, static type checker with `struct` fields and `enum`/`match` dispatch, and a deterministic C99 backend reaching a Stage 2 to Stage 3 SHA-256 fixed point.

`buildc_selfhost_wasm.bld` adds Slice 4: a direct zero-C binary WebAssembly (`.wasm` v1) encoder supporting `ULEB128` and `SLEB128`, Type/Function/Memory/Export/Code sections, `i64` arithmetic, `if`/`else`, `while`/`break`, direct and mutual recursion, and zero-import linear-memory output capture.

`buildc_selfhost_x64.bld` adds Slice 5: a direct zero-C native `x86-64` machine-code and Windows PE32+ executable emitter (DOS header, PE32+ COFF/Optional headers, ASLR/NX-compatible RIP-relative `.text`, and `.rdata` `KERNEL32.dll` `ExitProcess` import table).

`buildc_selfhost_cross.bld` adds Slice 6: a table-driven cross-OS backend sharing one code generator across `x64-pe`, `x64-elf`, and `wasm`.

`buildc_selfhost_io.bld` adds Slice 7: native stdout output (`WriteFile` on Windows PE32+ and `SYS_write` on Linux ELF64) plus fixed stack arrays `[i64; N]`.

`buildc_selfhost_quadarch.bld` adds Slices 8 and 9: a five-target, four-architecture zero-C compiler emitting standalone binaries for `x64-pe`, `x64-elf`, `rv64-elf` (RISC-V 64), `aarch64-elf` (ARM64), and `wasm` (WebAssembly 1.0).

## Zero-C Kernel and Micro-Compiler Payloads

`n55_jitter_kernel.bld` provides a bare-metal quantized GEMV, ReLU, and residual micro-kernel compiling to under 6 KB per native target.

`buildos_zeroc_kernel.bld` provides a zero-C capability table, A/B anti-downgrade, supervisor, and 16x16 framebuffer micro-kernel verifying fail-closed rejection of three embedded security attacks.

`buildc_micro_zeroc.bld` provides Slice 10: a zero-C micro-compiler emitting byte-identical raw `x86-64` machine code across all five host architectures.

## Verification

Check and lint every module with `buildc`:

```bash
BUILDLANG_STDLIB=stdlib buildc check universe/selfhost/buildc_selfhost_quadarch.bld
BUILDLANG_STDLIB=stdlib buildc lint universe/selfhost/buildc_selfhost_quadarch.bld
```

Build a Stage 1 binary with `buildc build` in release mode and invoke target modes `c`, `wasm`, `x64-pe`, `x64-elf`, `rv64-elf`, or `aarch64-elf` with input `.bld` and output binary paths.

## License

`buildc_selfhost_s3.bld` is licensed under `AGPL-3.0-only`, matching the root repository license. All Slice 4 through 10 files (`buildc_selfhost_wasm.bld`, `buildc_selfhost_x64.bld`, `buildc_selfhost_cross.bld`, `buildc_selfhost_io.bld`, `buildc_selfhost_quadarch.bld`, `buildc_micro_zeroc.bld`, `buildos_zeroc_kernel.bld`, and `n55_jitter_kernel.bld`) carry explicit `SPDX-License-Identifier: MIT` headers and are licensed under the MIT License (see `LICENSE`).
