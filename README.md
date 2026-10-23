<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/buildlang/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/buildlang/main/docs/art/hero-light.svg" alt="buildlang: Systems language with typed capability effects and native C output. A fine lattice of lines bulges outward around a bright core, as if seen through a lens, inside a ring." width="100%">
</picture>

# buildlang

Systems language with typed capability effects and native C output.

```bash
cargo install buildlang
```

[![version: 1.10.0](https://img.shields.io/badge/version-1.10.0-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/buildlang/releases/latest)
[![CI](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-AGPL--3.0--only-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/buildlang/blob/main/LICENSE)
![rust edition 2021](https://img.shields.io/badge/rust-edition_2021-e6e1d6?style=flat-square&labelColor=1a1712)

BuildLang compiles `.bld` source to native binaries through a C backend, emits
HLSL and GLSL for shader work, and carries experimental SPIR-V, LLVM IR,
WebAssembly, Rust, x86-64, and ARM64 backends. The type system pairs
Hindley-Milner inference with typed algebraic effects and an opt-in
experimental `#[linear]` attribute for no-cloning values. The `buildc` CLI
covers build, run, test, repl, fmt, pkg, watch, and doctor, plus a bundled LSP
server with completion, hover, diagnostics, and go-to-definition.

[Landing page](https://harperz9.github.io/buildlang/) | [Build ecosystem](https://github.com/HarperZ9/build-universe) | [VS Code extension](https://github.com/HarperZ9/buildlang-vscode) | [grammar](https://github.com/HarperZ9/buildlang-tmLanguage)

## See it work, step by step

The [animated explainer](https://harperz9.github.io/repo-explainers/buildlang.html)
shows an undeclared file read being rejected, a policy profile that denies it,
a check receipt catching an edit, HLSL from a fragment function, and a
heat-equation receipt that verifies while a forged one fails. Its source is
[docs/explainer/index.html](docs/explainer/index.html).

## Highlights

- **Typed capability effects.** Ambient access is part of a function's type.
  Calling `read_file` requires `~ FileSystem` in the signature, `tcp_connect`
  requires `~ Network`, an `extern` call requires `~ Foreign`, and
  compile-time macros like `include_str!` and `env!` are gated the same way.
  The checker tracks effects through function values, closures, struct fields,
  control flow, and async blocks, so a callback cannot hide one. See [docs/EFFECTS_GUIDE.md](docs/EFFECTS_GUIDE.md).
- **Native binaries through C.** The C backend is the production execution
  path: `buildc run` compiles to C, invokes your system C compiler (gcc,
  clang, or MSVC), and runs the result. One command, one binary.
- **Two-way C FFI.** `extern "C" link "sqlite3" header "<sqlite3.h>"` calls a
  third-party C-ABI library and links it in one build; `extern "C" fn`
  exports a BuildLang function with a stable symbol, and
  `buildc build --emit header` writes the matching `.h` for C and C++
  consumers. `--emit staticlib` builds a static library a C or C++ host
  links, with `#[repr(C)]` structs defined in the header.
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
  disciplines. The check is not yet complete: some escapes still compile. See
  [docs/LINEAR-TYPES.md](docs/LINEAR-TYPES.md) for what it catches.
- **Full toolchain in one binary.** `buildc` bundles build, run, check, test,
  repl, fmt, lint, pkg, watch, doctor, an LSP server, MIR and BDF utilities,
  and receipt tooling. No separate installs.
- **Build receipts.** `buildc check --receipt` writes the effects,
  capabilities and source digests of a check to JSON, and
  `buildc receipt verify` re-runs the check later and fails if the source
  changed. `buildc run --emit-receipt` does the same for a numeric program's
  output against a stated invariant.

## Install

From crates.io (installs the `buildc` binary, version 1.10.0):

```bash
cargo install buildlang
```

The standard library is compiled into `buildc`. When no `stdlib/` directory
is found beside the binary, `buildc` writes its copy to a versioned directory
under the system temp directory and uses that; `BUILDLANG_STDLIB` overrides
it. `buildc doctor` reports which one is in use. (1.5.0 and earlier shipped
without it and needed `BUILDLANG_STDLIB` pointed at a clone.)

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

`buildc check` lists the effects a program uses and can enforce a policy over
them. Check the hello program against the built-in `console-only`
profile and print a receipt:

```bash
buildc check examples/quickstart/hello.bld --profile console-only --receipt -
```

Output (excerpt, verified against buildc 1.2.0):

```text
Type checking... OK

No errors found in 'examples/quickstart/hello.bld'
{
  "schema": "buildlang-check-receipt/v1",
  "compiler_version": "1.2.0",
  "status": "passed",
  "declared_effects": { "main": ["Console"] },
  ...
}
```

<p align="center"><img src="docs/art/effect-lane.svg" alt="Eight stages from source to re-verify, ending in passed or rejected." width="100%"></p>

If the program also read a file, the check would fail until `main` declared
`~ FileSystem` and the policy allowed it. Built-in profiles: `pure`,
`console-only`, `offline`, `ci-review`, and `strict-accountability`
(`buildc policy list`). Save a receipt to a file and check it later with
`buildc receipt verify receipt.json --expect-profile ci-review`. If the source
changed since, verify fails and says why.

## The buildc CLI

| Command | Purpose |
|---|---|
| `buildc <file>` | Compile a file; `-o`, `--target`, `--stdio-mode native\|portable-lf`, `-O 0-3`, `-g` |
| `buildc run <file>` | Compile and run via the C backend; `--stdio-mode native\|portable-lf`, `--emit-receipt`, `--invariant`, `--units`, `--gpu` |
| `buildc build [path]` | Build a project; `--emit c\|header\|staticlib\|exe`, `--release`, `--target`, `--stdio-mode native\|portable-lf`, `--keep-c` |
| `buildc check <file>` | Type-check; `--error-format json`, `--receipt`, `--policy`, `--profile`, `--expect-profile-digest` |
| `buildc explain [code]` | Explain a diagnostic code such as `BL0403`, or list every code |
| `buildc test [dir]` | Run `.bld` programs against `.expected` files |
| `buildc fmt` / `buildc lint` | Format (`--check`, `--write`) and lint source |
| `buildc repl` | Interactive session |
| `buildc lsp` | Bundled LSP server (completion, hover, diagnostics with the same codes as `check`, go-to-definition, semantic tokens) |
| `buildc mcp` | The compiler as MCP tools over stdio: `check` and `explain`; it never runs programs |
| `buildc watch [path]` | Recompile on change (`--target spirv\|c`) |
| `buildc pkg` | Package manager (`init`, `add`, `resolve`, `search`) |
| `buildc mir emit\|load` | Emit or load the versioned `buildlang.mir/v0` JSON interlingua |
| `buildc bdf` | Build Data Format: `encode`, `decode`, `validate`, envelope bridges |
| `buildc policy list\|print\|scaffold` | Built-in check policy profiles |
| `buildc receipt verify\|export` | Re-check saved receipts; export witnessed measurement rows |
| `buildc corpus verify` | Verify the semantic corpus receipts and real C stdout |
| `buildc doctor` / `buildc version` | Toolchain diagnosis; version info |

Agents: start with [skills/buildlang/SKILL.md](skills/buildlang/SKILL.md) or [llms.txt](llms.txt).

Full flags: `buildc --help` and `buildc <command> --help`. The command
reference with expected output lives in [USAGE.md](USAGE.md).

## Backends

<p align="center"><img src="docs/art/backend-maturity.svg" alt="A record card titled Every backend you can select, and how far each one goes, listing the nine buildc --target flags with the rank buildc doctor gives each one. The c row is accented as the only primary backend, the one the substrate receipt records as the production anchor. hlsl and glsl are supported shader source output, and the remaining six are experimental." width="100%"></p>

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

`buildc corpus verify` runs an 8-program corpus through the C backend and
compares its output.

The generated C runtime defaults to native stdio behavior. On Windows, native
C stdio translates `\n` written through stdout or stderr to CRLF. Use
`--stdio-mode portable-lf` on the C backend when you need the same LF bytes on
every platform. Other targets reject the flag.

## Numeric output receipts

`buildc run --emit-receipt <path> --invariant <name>` records a numeric
program's stdout as a series, checks an invariant over it, and writes a JSON
receipt. `buildc receipt verify` re-runs the program and compares. Eight
invariants are built in: `energy-monotone`, `conservation`, `bounded`,
`energy-identity`, `relation`, `conserved-band`, `non-negative`, and
`cross_backend_columns_agree`. `--units m/s` normalizes a declared SI unit
before the receipt is written.

```bash
buildc run examples/heat_equation_energy.bld --emit-receipt receipt.json   --invariant energy-monotone --problem 1d-heat-equation-energy
buildc receipt verify receipt.json

# the 30-kernel example corpus, each passing kernel paired with one that must fail
buildc receipt corpus examples/scientific-corpus.json
```

<p align="center"><img src="docs/art/receipt-lane.svg" alt="Eight stages from kernel to re-derive, ending in verified, failed, or refused." width="100%"></p>

Receipts cover deterministic, exact-probabilistic, seeded stochastic, Monte
Carlo (`--mc-estimator`, `--mc-samples`, `--mc-interval`) and budgeted search
(`--budget-steps`, `--budget-consumed`) programs, plus a mode that runs one
kernel through the C and Rust backends and checks they agree.
`buildc receipt chain` bundles several receipts into one file. A receipt
covers one run of the compiled program. A program that uses the `Model`
capability cannot get one. With `--stdio-mode portable-lf` the mode is stored
in the receipt and used again on verify.

[docs/FIVE-MODES-TOUR.md](docs/FIVE-MODES-TOUR.md) runs every mode one command
at a time. Schema and flags:
[docs/SCIENTIFIC-RECEIPT.md](docs/SCIENTIFIC-RECEIPT.md) and
[docs/DIMENSIONAL-ANALYSIS.md](docs/DIMENSIONAL-ANALYSIS.md).

`buildc receipt verify` also reads two receipt kinds written by other tools:
model boundary receipts (`buildlang-model-boundary-receipt/v0`) from a model
harness shim, and Flywheel tool-call receipts (`flywheel.tool-call-receipt/v1`).

## Status

BuildLang 1.10.0. The C backend, effect checking, HLSL and GLSL output, and
receipts are the supported core. SPIR-V, LLVM IR, WASM, Rust, x86-64, ARM64, GPU dispatch
and `#[linear]` types are experimental. Release test results are in
[STATUS.md](STATUS.md) and changes in [CHANGELOG.md](CHANGELOG.md).

## Documentation and ecosystem

- [docs/INTRODUCTION.md](docs/INTRODUCTION.md): what BuildLang is and your first ten minutes
- [USAGE.md](USAGE.md): full command reference with verified output
- [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md): tutorial from install to shaders
- [docs/EFFECTS_GUIDE.md](docs/EFFECTS_GUIDE.md): the capability-effect system
- [docs/LINEAR-TYPES.md](docs/LINEAR-TYPES.md): linear types
- [DESIGN.md](DESIGN.md) and [ARCHITECTURE.md](ARCHITECTURE.md): pipeline and rationale
- Peers: [build-universe](https://github.com/HarperZ9/build-universe),
  [buildlang-vscode](https://github.com/HarperZ9/buildlang-vscode),
  [buildlang-tmLanguage](https://github.com/HarperZ9/buildlang-tmLanguage)

<p align="center"><img src=".github/assets/banner.svg" alt="buildlang: A real systems language: typed capability effects, sum and linear types, C FFI, native binaries." width="100%"></p>

Contributor checks before changing public behavior: `cargo test` and
`cargo fmt --check` from `compiler/`, `buildc doctor`, and
`buildc corpus verify`. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Text: CC BY 4.0. Code: AGPL-3.0-only.

The paper in [`papers/`](papers/) is licensed CC BY 4.0, the same licence its
Zenodo deposit records. Share and adapt it with credit to Zain Dana Harper; the
terms are in [`LICENSE-TEXT`](LICENSE-TEXT).

The compiler, toolchain and their documentation are under the GNU Affero General
Public License, version 3 only (AGPL-3.0-only). You may use, study, change and
share them. If you run a changed version as a network service, you must offer its
users the source of that version. See [LICENSE](LICENSE).

Releases up to and including 1.8.0 were published under the BuildLang Fair-Source
License v1.0 and keep it. AGPL-3.0-only applies from the next release.

### Generated output

The code BuildLang generates from your source may be shipped inside your programs and games,
under terms of your choosing. This covers the generated C, the C header built into it, and
the other code `buildc` generates, such as HLSL and GLSL. AGPL-3.0-only covers the compiler
and toolchain. As an additional permission under section 7 of that licence, its terms do not
extend to output that `buildc` generates, including the parts of the compiler's runtime and
header it copies into that output. This statement sits beside [LICENSE](LICENSE) and does not
change its text.
