<p align="center"><img src=".github/assets/banner.svg" alt="buildlang: A real systems language: typed capability effects, sum and linear types, C FFI, native binaries." width="100%"></p>

**A real systems language: typed capability effects, sum and linear types, C FFI, native binaries.**

[![crates.io](https://img.shields.io/crates/v/buildlang?style=flat-square&labelColor=14041b&color=ff8334)](https://crates.io/crates/buildlang/)
[![license: BuildLang Fair-Source](https://img.shields.io/badge/license-BuildLang_Fair--Source-8f8095?style=flat-square&labelColor=14041b)](LICENSE)
[![downloads](https://img.shields.io/crates/dv/buildlang?label=downloads&style=flat-square&labelColor=14041b)](https://crates.io/crates/buildlang/)
![rust](https://img.shields.io/badge/rust-edition_2021-orange?style=flat-square&labelColor=14041b)
[![CI](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml)
[![part of: Build ecosystem](https://img.shields.io/badge/part_of-Build_ecosystem-00b3a4?style=flat-square&labelColor=14041b)](https://github.com/HarperZ9/build-universe)

BuildLang compiles `.bld` source to native binaries through a C backend, emits
HLSL and GLSL for shader work, and carries experimental SPIR-V, LLVM IR,
WebAssembly, Rust, x86-64, and ARM64 backends. The type system pairs
Hindley-Milner inference with typed algebraic effects and an opt-in
experimental `#[linear]` attribute for no-cloning values. The `buildc` CLI
covers build, run, test, repl, fmt, pkg, watch, and doctor, plus a bundled LSP
server with completion, hover, diagnostics, and go-to-definition. Every checked
build can write a receipt you can re-check.

[Landing page](https://harperz9.github.io/buildlang/) | [Build ecosystem](https://github.com/HarperZ9/build-universe) | [VS Code extension](https://github.com/HarperZ9/buildlang-vscode) | [grammar](https://github.com/HarperZ9/buildlang-tmLanguage)

## Highlights

- **Typed capability effects.** Ambient access is part of a function's type.
  Calling `read_file` requires `~ FileSystem` in the signature, `tcp_connect`
  requires `~ Network`, an `extern` call requires `~ Foreign`, and
  compile-time macros like `include_str!` and `env!` are gated the same way.
  The checker tracks effects through function values, closures, struct fields,
  control flow, and async blocks, so a callback cannot silently launder a
  capability. See [docs/EFFECTS_GUIDE.md](docs/EFFECTS_GUIDE.md).
- **Native binaries through C.** The C backend is the production execution
  path: `buildc run` compiles to C, invokes your system C compiler (gcc,
  clang, or MSVC), and runs the result. One command, one binary.
- **Two-way C FFI.** `extern "C" link "sqlite3" header "<sqlite3.h>"` calls a
  third-party C-ABI library and links it in one build; `extern "C" fn`
  exports a BuildLang function with a stable symbol, and
  `buildc build --emit header` writes the matching `.h` for C and C++
  consumers.
- **Shader output.** `#[fragment]` functions compile directly to HLSL (for
  ReShade and DirectX) or GLSL (for OpenGL and Vulkan). See
  [docs/SHADER_GUIDE.md](docs/SHADER_GUIDE.md).
- **GPU compute (experimental).** `#[compute]` kernels compile to
  dispatchable SPIR-V validated by spirv-val, and a build with
  `--features gpu` adds `buildc run --gpu`, which dispatches the kernel on a
  physical Vulkan device and cross-checks the readback against the CPU
  scalar loop. Workgroup shared memory and barriers are supported.
- **Experimental linear types.** An opt-in `#[linear]` attribute marks a
  struct or enum as no-cloning: a value should be moved at most once, the
  shared foundation of qubit, no-double-spend, and resource-handle
  disciplines. It rejects a large regression-tested set of escapes but is not
  yet fully sound; honest scope in
  [docs/LINEAR-TYPES.md](docs/LINEAR-TYPES.md).
- **Full toolchain in one binary.** `buildc` bundles build, run, check, test,
  repl, fmt, lint, pkg, watch, doctor, an LSP server, MIR and BDF utilities,
  and receipt tooling. No separate installs.
- **Re-checkable receipts.** `buildc check --receipt` seals what a build
  observed (effects, capabilities, source digests) into JSON that
  `buildc receipt verify` re-derives later; a second receipt family witnesses
  numeric program output against stated invariants.

## Install

From crates.io (installs the `buildc` binary):

```bash
cargo install buildlang
```

> Previously published as `quantalang`; that crate is deprecated and points
> here. Use `buildlang` / `buildc`.

Or build from the repository source:

```bash
cd compiler
cargo build --release
```

Add `compiler/target/release/buildc` (`buildc.exe` on Windows) to your PATH,
then verify the local toolchain (C compiler, stdlib, optional backend tools):

```bash
buildc doctor
```

## Quick start

Create `hello.bld`. `println!` is a `Console` capability, so `main` declares
the effect:

```build
fn main() ~ Console {
    println!("Hello from BuildLang!");
}
```

Compile and run through the C backend:

```bash
buildc run hello.bld
# Hello from BuildLang!
```

The repository ships tested quickstart programs:

```bash
buildc run examples/quickstart/hello.bld
buildc run examples/quickstart/ledger.bld          # prints: balance: 115
buildc run examples/quickstart/effects_greeting.bld
buildc examples/quickstart/vignette_shader.bld --target hlsl -o vignette.hlsl
```

Or emit C and build it yourself:

```bash
buildc hello.bld -o hello.c
cc hello.c -o hello && ./hello
```

## Worked example: effects and a policy gate

`buildc check` reports the capability surface of a program and can enforce a
policy over it. Check the hello program against the built-in `console-only`
profile and print a receipt:

```bash
buildc check examples/quickstart/hello.bld --profile console-only --receipt -
```

Output (excerpt, verified against buildc 1.1.0):

```
Type checking... OK

No errors found in 'examples/quickstart/hello.bld'
{
  "schema": "buildlang-check-receipt/v1",
  "compiler_version": "1.1.0",
  "status": "passed",
  "declared_effects": { "main": ["Console"] },
  ...
}
```

If the program also read a file, the check would fail until `main` declared
`~ FileSystem` and the policy allowed it. Built-in profiles: `pure`,
`console-only`, `offline`, `ci-review`, and `strict-accountability`
(`buildc policy list`). Save a receipt to a file and re-verify it later with
`buildc receipt verify receipt.json --expect-profile ci-review`; verification
re-runs the check against current source bytes and digests, so drift fails
with a typed reason.

## The buildc CLI

| Command | Purpose |
|---|---|
| `buildc <file>` | Compile a file; `-o`, `--target`, `-O 0-3`, `-g` |
| `buildc run <file>` | Compile and run via the C backend; `--emit-receipt`, `--invariant`, `--units`, `--gpu` |
| `buildc build [path]` | Build a project; `--emit c\|header\|exe`, `--release`, `--target`, `--keep-c` |
| `buildc check <file>` | Type-check; `--receipt`, `--policy`, `--profile`, `--expect-profile-digest` |
| `buildc test [dir]` | Run `.bld` programs against `.expected` files |
| `buildc fmt` / `buildc lint` | Format (`--check`, `--write`) and lint source |
| `buildc repl` | Interactive session |
| `buildc lsp` | Bundled LSP server (completion, hover, diagnostics, go-to-definition, semantic tokens) |
| `buildc watch [path]` | Recompile on change (`--target spirv\|c`) |
| `buildc pkg` | Package manager (`init`, `add`, `resolve`, `search`) |
| `buildc mir emit\|load` | Emit or load the versioned `buildlang.mir/v0` JSON interlingua |
| `buildc bdf` | Build Data Format: `encode`, `decode`, `validate`, envelope bridges |
| `buildc policy list\|print\|scaffold` | Built-in check policy profiles |
| `buildc receipt verify\|export` | Re-check saved receipts; export witnessed measurement rows |
| `buildc corpus verify` | Verify the semantic corpus receipts and real C stdout |
| `buildc doctor` / `buildc version` | Toolchain diagnosis; version info |

Full flags: `buildc --help` and `buildc <command> --help`. The command
reference with expected output lives in [USAGE.md](USAGE.md).

## Backends

| Target | Flag | Output | Status |
|---|---|---|---|
| C | `--target c` (default) | `.c` / executable | Production |
| HLSL | `--target hlsl` | `.hlsl` | Working |
| GLSL | `--target glsl` | `.glsl` | Working |
| SPIR-V | `--target spirv` | `.spv` | Experimental (compute kernels validate under spirv-val) |
| LLVM IR | `--target llvm` | `.ll` | Experimental |
| WASM | `--target wasm` | `.wasm` | Experimental |
| Rust | `--target rust` | `.rs` | Experimental (subset, validated with rustc) |
| x86-64 | `--target x86-64` | `.o` | Experimental |
| ARM64 | `--target arm64` | `.o` | Experimental |

An 8-program semantic corpus pins C-backend behavior: `buildc corpus verify`
checks the manifest, the C and Rust execution receipts, and real C-backend
stdout together.

## Scientific-runtime receipts

For numeric programs, `buildc run --emit-receipt <path> --invariant <name>`
captures the program's numeric stdout as a measurement series, checks a
stated invariant over it, and seals a re-checkable JSON receipt that
`buildc receipt verify` re-derives by re-running the program. Seven
invariants ship (`energy-monotone`, `conservation`, `bounded`,
`energy-identity`, `relation`, `conserved-band`, `non-negative`), each with a
paired negative-fixture kernel that must fail for the right reason.
`--units m/s` canonicalizes a declared physical unit through a dependency-free
SI dimensional-analysis core before sealing. Honest scope: the receipt
witnesses the observed output series, not the model or any physical law.
Details: [docs/SCIENTIFIC-RECEIPT.md](docs/SCIENTIFIC-RECEIPT.md) and
[docs/DIMENSIONAL-ANALYSIS.md](docs/DIMENSIONAL-ANALYSIS.md).

## Status and maturity

BuildLang 1.1.x. The C backend, capability-effect checking, HLSL/GLSL
output, and the receipt tooling are the verified core; SPIR-V, LLVM IR, WASM,
Rust, x86-64, ARM64, GPU dispatch, and `#[linear]` types are labeled
experimental and stay that way until their evidence says otherwise. The
release-shaped baseline (2026-07-02, local `cargo test` from `compiler/`):
lib 940, bin 135, cli 307, lexer 52, parser 88 passing, 0 failing, with
`buildc corpus verify` 8/8. Ground-truth release evidence lives in
[STATUS.md](STATUS.md); [CHANGELOG.md](CHANGELOG.md) tracks changes.

## Documentation and ecosystem

- [docs/INTRODUCTION.md](docs/INTRODUCTION.md): what BuildLang is and your first ten minutes
- [USAGE.md](USAGE.md): full command reference with verified output
- [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md): tutorial from install to shaders
- [docs/EFFECTS_GUIDE.md](docs/EFFECTS_GUIDE.md): the capability-effect system
- [docs/LINEAR-TYPES.md](docs/LINEAR-TYPES.md): linear types, enforced vs open
- [DESIGN.md](DESIGN.md) and [ARCHITECTURE.md](ARCHITECTURE.md): pipeline and rationale
- Peers: [build-universe](https://github.com/HarperZ9/build-universe),
  [buildlang-vscode](https://github.com/HarperZ9/buildlang-vscode),
  [buildlang-tmLanguage](https://github.com/HarperZ9/buildlang-tmLanguage)

Contributor checks before changing public behavior: `cargo test` and
`cargo fmt --check` from `compiler/`, `buildc doctor`, and
`buildc corpus verify`. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Why receipts

Every claim above that could drift, backend maturity, corpus behavior,
capability surfaces, numeric invariants, is backed by a receipt a third party
can re-check with one command. That is the design stance: evidence you can
re-run beats assertions you have to trust.

## License

BuildLang Fair-Source License v1.0, source-available, not open source: read
it, run it, build on it; commercial use that competes with the project is
reserved. See [LICENSE](LICENSE).
