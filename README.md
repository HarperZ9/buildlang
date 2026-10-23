<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/buildlang/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/buildlang/main/docs/art/hero-light.svg" alt="buildlang: Systems language with typed capability effects and native C output. A fine lattice of lines bulges outward around a bright core, as if seen through a lens, inside a ring." width="100%">
</picture>

# buildlang

A compiled language where each function's type lists the side effects it may
perform, such as reading files, opening sockets or calling C.

```bash
cargo install buildlang
```

[![version: 1.7.0](https://img.shields.io/badge/version-1.7.0-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/buildlang/releases/latest)
[![CI](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-BuildLang_Fair--Source-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/buildlang/blob/main/LICENSE)
![rust edition 2021](https://img.shields.io/badge/rust-edition_2021-e6e1d6?style=flat-square&labelColor=1a1712)

You write `.bld` files and compile them with `buildc`. The compiler turns your
program into C and hands that C to gcc, clang or MSVC, so `buildc run` gives
you a native binary in one step. A function that reads a file must say
`~ FileSystem` in its signature. One that opens a socket says `~ Network`, and
one that calls into C says `~ Foreign`. Leave an effect out and the program
does not compile. On top of that, `buildc check --profile` can forbid a
capability for a whole program, so a CI job can refuse any code that touches
the network. The same source language also compiles small fragment functions
to HLSL or GLSL for shaders.

It is for people who write native tools, plugins or shader code and want the
compiler to show and limit what each part of a program can touch. It also
suits anyone who wants to try an effect system in a language that ends up as
plain C.

[Landing page](https://harperz9.github.io/buildlang/) | [Build ecosystem](https://github.com/HarperZ9/build-universe) | [VS Code extension](https://github.com/HarperZ9/buildlang-vscode) | [grammar](https://github.com/HarperZ9/buildlang-tmLanguage)

## See it work, step by step

The [animated explainer](https://harperz9.github.io/repo-explainers/buildlang.html)
walks through an undeclared file read being rejected, a policy profile that
denies it, a saved check report catching a later edit, HLSL from a fragment
function, and a numeric run whose saved report checks out while a forged one
fails. Every value on it is output from this repository. Its source is
[docs/explainer/index.html](docs/explainer/index.html).

## What you get

- **Effects in the type.** Calling `read_file` requires `~ FileSystem` in the
  signature, `tcp_connect` requires `~ Network`, an `extern` call requires
  `~ Foreign`, and compile-time macros like `include_str!` and `env!` are
  gated the same way. The checker follows effects through function values,
  closures, struct fields, control flow and async blocks, so passing a
  callback around does not hide what it does. You can also declare your own
  effects and handle them with `handle`/`with`. Handlers are one-shot today:
  after a handler runs, control continues after the whole `handle` block and
  never resumes at the `perform`. See [docs/EFFECTS_GUIDE.md](docs/EFFECTS_GUIDE.md).
- **Native binaries through C.** The C backend is the one production path:
  `buildc run` compiles to C, invokes your system C compiler and runs the
  result.
- **Two-way C FFI.** `extern "C" link "sqlite3" header "<sqlite3.h>"` calls a
  C library and links it in the same build (`examples/ffi/main.bld`).
  `extern "C" fn` exports a BuildLang function with a stable symbol, and
  `buildc build --emit header` writes the matching `.h` for C and C++ callers.
- **Shader output.** `#[fragment]` functions compile to HLSL (for ReShade and
  DirectX) or GLSL (for OpenGL and Vulkan). See
  [docs/SHADER_GUIDE.md](docs/SHADER_GUIDE.md).
- **A capability report you can save.** `buildc check --receipt file.json`
  writes the effects each function declared, the capabilities it used and a
  SHA-256 of every source file. `buildc receipt verify` re-runs the check
  later and fails with a named reason when the source or the policy changed.
- **One binary for the toolchain.** `buildc` bundles build, run, check, test,
  repl, fmt, lint, pkg, watch, doctor and an LSP server with completion, hover,
  diagnostics and go-to-definition.
- **GPU compute, experimental.** `#[compute]` kernels compile to SPIR-V that
  passes `spirv-val`. A build with `--features gpu` adds `buildc run --gpu`,
  which runs the kernel on a Vulkan device and compares the result with the
  CPU loop. A plain `cargo install buildlang` leaves this feature out.
- **Linear types, experimental.** `#[linear]` marks a struct or enum as a
  value that may be moved at most once. It rejects a large tested set of
  escapes and is not yet sound; [docs/LINEAR-TYPES.md](docs/LINEAR-TYPES.md)
  lists what it does and does not catch.

## Limits

- Only the C backend is production quality. HLSL and GLSL output works for
  shader functions. SPIR-V, LLVM IR, WebAssembly, Rust, x86-64 and ARM64
  output are experimental, and `buildc doctor` prints each backend's level.
- Effect handlers do not resume (see above), so patterns such as generators
  or async built from resumable handlers are out of reach for now.
- `buildc pkg` works with local and path packages. The public package
  registry does not exist yet ([docs/PACKAGES.md](docs/PACKAGES.md)).
- The standard library is small: maths, strings, iterators, option and result
  types, I/O, sorting and linear algebra (`stdlib/`).
- The compiler is under the BuildLang Fair-Source License, which is
  source-available and reserves competing commercial use. Code that `buildc`
  generates from your source is yours to ship (see [License](#license)).

## How it compares

- **Koka** and **Effekt** also put effects in function types, and Koka also
  compiles to C. Both support resumable handlers, which BuildLang does not.
  BuildLang's effects centre on built-in host capabilities such as files,
  network, processes, FFI and GPU, with program-wide policy profiles and a
  saved check report.
- **Nim** compiles to C and infers effects, which you can restrict with the
  `raises` and `tags` pragmas on the procs you choose. BuildLang requires every
  function that touches the host to declare it.
- **OCaml 5** has effect handlers that do not appear in function types, so the
  compiler does not tell you which functions perform I/O.
- **Rust** and **Zig** give you native code and C interop with no effect
  tracking for I/O. They are far more mature, with large ecosystems that
  BuildLang does not have.
- **Deno** enforces file and network permissions at run time for JavaScript.
  BuildLang checks the same kind of rule at compile time.

## Install

From crates.io (installs the `buildc` binary, version 1.7.0). You also need a C
compiler on your PATH (gcc, clang or MSVC) for `buildc run`:

```bash
cargo install buildlang
buildc doctor
```

`buildc doctor` reports the C compiler it found, the standard library in use
and each backend's maturity. The standard library is compiled into `buildc`.
When no `stdlib/` directory is found beside the binary, `buildc` writes its
copy to a versioned directory under the system temp directory and uses that;
`BUILDLANG_STDLIB` overrides it. (1.5.0 and earlier shipped without it and
needed `BUILDLANG_STDLIB` pointed at a clone.)

> Previously published as `quantalang`; that crate is deprecated and points
> here. Use `buildlang` / `buildc`.

Or build from the repository source:

```bash
cd compiler
cargo build --release
```

Then add `compiler/target/release/buildc` (`buildc.exe` on Windows) to your
PATH.

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

Now add a file read without declaring it:

```build fail=BL0403
fn main() ~ Console {
    let s = read_file("x.txt");
    println!("{}", s);
}
```

```text
$ buildc check bad.bld
Type errors found:
  1:1: [BL0403] function performs undeclared effect `FileSystem`
  help: add `FileSystem` to the function's effect row: `~ Console + FileSystem`
```

From a clone of this repository, the tested quickstart programs:

```bash
buildc run examples/quickstart/hello.bld
buildc run examples/quickstart/ledger.bld          # prints: balance: 115
buildc run examples/quickstart/effects_greeting.bld  # prints: Hello, teammate!
buildc examples/quickstart/vignette_shader.bld --target hlsl -o vignette.hlsl
```

Or emit C and build it yourself. The generated C uses the maths library, and
on Windows its socket code needs Winsock:

```bash
buildc hello.bld -o hello.c
cc hello.c -o hello -lm && ./hello        # Linux
gcc hello.c -o hello.exe -lws2_32         # Windows with MinGW
```

## Worked example: effects and a policy gate

`buildc check` reports the capability surface of a program and can enforce a
policy over it. Check the hello program against the built-in `console-only`
profile and print a receipt:

```bash
buildc check examples/quickstart/hello.bld --profile console-only --receipt -
```

Output (excerpt, buildc 1.7.0):

```text
Type checking... OK

No errors found in 'examples/quickstart/hello.bld'
{
  "schema": "buildlang-check-receipt/v1",
  "compiler_version": "1.7.0",
  "status": "passed",
  "declared_effects": { "main": ["Console"] },
  ...
}
```

<p align="center"><img src="docs/art/effect-lane.svg" alt="Eight stages from source to re-verify, ending in passed or rejected." width="100%"></p>

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
| `buildc <file>` | Compile a file; `-o`, `--target`, `--stdio-mode native\|portable-lf`, `-O 0-3`, `-g` |
| `buildc run <file>` | Compile and run via the C backend; `--stdio-mode native\|portable-lf`, `--emit-receipt`, `--invariant`, `--units`, `--gpu` |
| `buildc build [path]` | Build a project; `--emit c\|header\|exe`, `--release`, `--target`, `--stdio-mode native\|portable-lf`, `--keep-c` |
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

An 8-program semantic corpus pins C-backend behavior: `buildc corpus verify`
checks the manifest, the C and Rust execution receipts, and real C-backend
stdout together.

The generated C runtime defaults to native stdio behavior. On Windows, native
C stdio translates `\n` written through stdout or stderr to CRLF. Use
`--stdio-mode portable-lf` on the C backend when a receipt or a test
needs byte-stable LF output across platforms. The portable mode is rejected for
non-C targets rather than ignored.

## Scientific-runtime receipts

`buildc run --emit-receipt <path> --invariant <name>` captures a numeric
program's stdout as a measurement series, checks a stated invariant over it,
and seals a re-checkable JSON receipt; `buildc receipt verify` re-derives it
by re-running the program. The invariant family has eight members
(`energy-monotone`, `conservation`, `bounded`, `energy-identity`, `relation`,
`conserved-band`, `non-negative`, `cross_backend_columns_agree`), each with a
paired negative-fixture kernel that must fail for the right reason.
`--units m/s` canonicalizes a declared physical unit through a
dependency-free SI dimensional-analysis core before sealing.
If `--stdio-mode portable-lf` is used, the receipt seals that mode and
verification replays it. Native mode is the default and is omitted from the
receipt so older missing-field receipts continue to verify as native.

The same schema seals five distinct computation modes end to end:
deterministic, exact-probabilistic (a quantum amplitude), seeded stochastic
(the `Random` capability, witnessed seed), Monte Carlo (`--mc-estimator` /
`--mc-samples` / `--mc-interval`, sealing the estimator's declared
denominator and interval method), and budgeted heuristic search
(`--budget-steps` / `--budget-consumed`, sealing the step ceiling and a
derived exhaustion flag), plus a cross-backend bonus mode that runs one
kernel through both the C and Rust backends and checks their agreement.
`buildc receipt chain` binds any number of receipts into one ordered,
tamper-evident bundle. A `Model` capability exists for calling out to a
model over TCP; the receipt layer refuses outright to emit or verify a
receipt over a Model-observing program, because models propose and oracles
dispose.

<p align="center"><img src="docs/art/receipt-lane.svg" alt="Eight stages from kernel to re-derive, ending in verified, failed, or refused." width="100%"></p>

The mode is not decoration on the seal. A Monte Carlo receipt carries its
estimator, its sample count, and the interval method that produced the interval;
a budgeted-search receipt carries the step ceiling and whether the search
exhausted it. A number without the terms it was measured under is not a result,
so those terms are sealed beside it rather than left in the prose around it.

Run it now:

```bash
# emit a receipt from the flagship kernel, then verify it
buildc run examples/heat_equation_energy.bld --emit-receipt receipt.json \
  --invariant energy-monotone --problem 1d-heat-equation-energy
buildc receipt verify receipt.json

# run the full 30-member example corpus (paired PASS / FAIL_EXPECTED kernels)
buildc receipt corpus examples/scientific-corpus.json

# emit one receipt per mode (docs/FIVE-MODES-TOUR.md has all six commands),
# then chain and verify the bundle
buildc receipt chain build det.json prob.json stoch.json mc.json heur.json cross.json -o chain.json
buildc receipt chain verify chain.json
```

Walk all five modes plus the cross-backend bonus one command at a time:
[docs/FIVE-MODES-TOUR.md](docs/FIVE-MODES-TOUR.md). Full schema, flags, and
failure-class reference:
[docs/SCIENTIFIC-RECEIPT.md](docs/SCIENTIFIC-RECEIPT.md) and
[docs/DIMENSIONAL-ANALYSIS.md](docs/DIMENSIONAL-ANALYSIS.md).

Honest scope: a receipt witnesses that the compiled program's observed
output series satisfies the stated invariant over that one run, never a
physical law (`NOT_A_NEW_PHYSICAL_LAW`); a budgeted-search receipt reports an
incumbent under its declared step ceiling and never claims optimality
(`NOT_PROVES_OPTIMALITY`); a Monte Carlo receipt seals the estimator's
declaration discipline (denominator, seed, interval method), not the
correctness of the interval.

## Status and maturity

BuildLang 1.7.0. The C backend, capability-effect checking, HLSL/GLSL
output, and the receipt tooling are the verified core; SPIR-V, LLVM IR, WASM,
Rust, x86-64, ARM64, GPU dispatch, and `#[linear]` types are labeled
experimental and stay that way until their evidence says otherwise. The
release-shaped baseline (2026-10-15, local `cargo test` from `compiler/`
with `RUSTFLAGS=-Dwarnings`): 1,892 tests passing, 0 failing (11 ignored).
The corpus is 30 `.bld` example kernels and 8 C-execution receipts.
Ground-truth release evidence lives in
[STATUS.md](STATUS.md); [CHANGELOG.md](CHANGELOG.md) tracks changes.

## Boundary receipts

BuildLang verifies computation through sealed, re-checkable receipts. Two
boundary receipt kinds extend this discipline beyond the compiler itself:

- **Model boundary receipts** (`buildlang-model-boundary-receipt/v0`): witness
  that a `Model`-capability call crossed the boundary between the harness and a
  model daemon. The harness-side shim emits the receipt; `buildc receipt verify`
  reads it. A model-observing program is refused a scientific receipt with
  `CAPABILITY_INADMISSIBLE` (models propose, oracles dispose).
- **Tool-call receipts** (`flywheel.tool-call-receipt/v1`): witness that an agent
  tool invocation occurred, binding the tool name, capability class (read / write
  / exec / external-mcp), admission decision, and witnessed args/output digests.
  The Flywheel harness emits these; `buildc receipt verify` reads them with a
  golden-pinned cross-language seal contract (Rust serde_json agrees byte-for-byte
  with Python json.dumps). This is the enforced AgentRiskBOM primitive.

Both share a seal idiom and the closed failure taxonomy
(`MALFORMED`, `SEAL_MISMATCH`, `DIGEST_MALFORMED`, `FIELD_CONTRACT_VIOLATION`)
with the scientific receipt verifier, and compose into receipt chains.

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

<p align="center"><img src=".github/assets/banner.svg" alt="buildlang: A real systems language: typed capability effects, sum and linear types, C FFI, native binaries." width="100%"></p>

The mark at the top of this page is generated from the repository name, so every
tool in this family carries a different one. The banner above is buildlang's own,
drawn by hand and kept. The mark and the two diagrams on this page are rendered
from `docs/art/buildlang.art.json` by `tools/render_repo_art.py`, and a gate
re-renders each drawing and compares it against what is committed, so a label
edited without a re-render fails CI instead of shipping.

Contributor checks before changing public behavior: `cargo test` and
`cargo fmt --check` from `compiler/`, `buildc doctor`, and
`buildc corpus verify`. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Why receipts

Every claim above that could drift, backend maturity, corpus behavior,
capability surfaces, numeric invariants, is backed by a receipt a third party
can re-check with one command. That is the design stance: evidence you can
re-run beats assertions you have to trust.

## License

Text: CC BY 4.0. Code: BuildLang Fair-Source v1.0.

The paper in [`papers/`](papers/) is licensed CC BY 4.0, the same licence its
Zenodo deposit records. Share and adapt it with credit to Zain Dana Harper; the
terms are in [`LICENSE-TEXT`](LICENSE-TEXT).

The compiler, toolchain and their documentation are under the BuildLang
Fair-Source License v1.0, source-available, not open source: read it, run it,
build on it; commercial use that competes with the project is reserved. See
[LICENSE](LICENSE).

### Generated output

The code BuildLang generates from your source may be shipped inside your programs and games,
under terms of your choosing. This covers the generated C, the C header built into it, and
the other code `buildc` generates, such as HLSL and GLSL. The Fair-Source License covers the
compiler and toolchain; it does not restrict what you ship that they generated. This
statement sits beside [LICENSE](LICENSE) and does not change its text.
