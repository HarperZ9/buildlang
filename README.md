# QuantaLang

[![crates.io](https://img.shields.io/crates/v/quantalang.svg)](https://crates.io/crates/quantalang)
[![docs.rs](https://img.shields.io/docsrs/quantalang)](https://docs.rs/quantalang)
[![VS Code Marketplace](https://img.shields.io/visual-studio-marketplace/v/HarperZ9.quantalang?label=VS%20Code)](https://marketplace.visualstudio.com/items?itemName=HarperZ9.quantalang)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**The Effects Language** - a Rust-built compiler for typed effects, systems
experiments, and shader-oriented code generation.

QuantaLang compiles `.quanta` source files to **C** as the primary verified
execution path, emits **HLSL** and **GLSL** for shader work, and keeps SPIR-V,
LLVM IR, WebAssembly, Rust source, x86-64, and ARM64 backends labeled as
experimental research surfaces.

**Landing page:** [harperz9.github.io/quantalang](https://harperz9.github.io/quantalang/)

## Install

From crates.io (recommended):

```bash
cargo install quantalang
# binary: quantac
```

Or build from source:

```bash
cd compiler
cargo build --release
```

Add `target/release/quantac` (or `target\release\quantac.exe` on Windows) to your PATH.

Verify your local toolchain:

```bash
quantac doctor
```

`doctor` reports the installed compiler version, C-backend readiness, stdlib and
local registry discovery, optional backend tools, and the current backend
maturity table.

## Editor support

Install the **[QuantaLang VS Code extension](https://marketplace.visualstudio.com/items?itemName=HarperZ9.quantalang)** - syntax highlighting, brackets, comment toggles. Grammar source: [HarperZ9/quantalang-tmLanguage](https://github.com/HarperZ9/quantalang-tmLanguage).

## Quick Start

Create `hello.quanta`:

```
fn main() {
    println!("Hello, World!");
}
```

Compile and run:

```bash
quantac run hello.quanta
```

The repository also carries tested quickstart examples:

```bash
quantac run examples/quickstart/hello.quanta
quantac run examples/quickstart/ledger.quanta
quantac run examples/quickstart/effects_greeting.quanta
quantac examples/quickstart/vignette_shader.quanta --target hlsl -o vignette_shader.hlsl
```

Or compile to C and build manually:

```bash
quantac hello.quanta -o hello.c
cc hello.c -o hello
./hello
```

## Shader Example

QuantaLang can compile shader code directly to HLSL or GLSL. Create `vignette.quanta`:

```
fn vignette(uv_x: f64, uv_y: f64, strength: f64, softness: f64) -> f64 {
    let dx = uv_x - 0.5;
    let dy = uv_y - 0.5;
    let dist = sqrt(dx * dx + dy * dy);
    let vig = smoothstep(0.5, 0.5 * softness, dist);
    1.0 - strength * (1.0 - vig)
}

#[fragment]
fn PS_Vignette(uv: vec2) -> vec4 {
    let color = tex2d(uv);
    let vig = vignette(color.x, color.y, 0.5, 0.6);
    vec4(color.x * vig, color.y * vig, color.z * vig, 1.0)
}
```

Compile to HLSL (for ReShade / DirectX):

```bash
quantac vignette.quanta --target hlsl -o vignette.fx
```

Compile to GLSL (for OpenGL / Vulkan):

```bash
quantac vignette.quanta --target glsl -o vignette.glsl
```

## CLI Commands

| Command         | Description                          |
|-----------------|--------------------------------------|
| `quantac lex`   | Tokenize a file and print tokens     |
| `quantac parse` | Parse a file and print the AST       |
| `quantac check <file> [--receipt PATH|-] [--policy policy.json|--profile NAME]` | Type-check, optionally evaluate policy, and optionally emit a JSON accountability receipt |
| `quantac build` | Build a project                      |
| `quantac run`   | Compile and run a `.quanta` file     |
| `quantac doctor` | Diagnose local toolchain readiness  |
| `quantac policy list [--json]` / `quantac policy print <name>` | List or emit built-in check policy profiles |
| `quantac receipt verify <receipt.json> [--source PATH] [--json]` | Re-check a saved accountability receipt against current source inputs |
| `quantac corpus verify [--root DIR] [--write]` | Verify semantic corpus receipts and C stdout; optionally refresh the C receipt |

## Capability Effects

`quantac check` now treats direct ambient runtime access as typed effects. A
function that calls helpers such as `read_file`, `write_file`, `tcp_connect`,
`process_exit`, `getenv`, `clock_ms`, Vulkan runtime helpers, or an `extern`
function must declare the matching capability effect in its signature:

```quanta
fn load_config() ~ FileSystem {
    read_file("ops.toml");
}

extern "C" { fn touch(); }

fn call_foreign() ~ Foreign {
    touch();
}
```

If the effect is missing, the checker reports the required capability and a
diagnostic note naming the ambient call or macro that triggered it. This is the
first security gate for practical ops/accountability use: file, network,
process, environment, clock, GPU, console helper/macro, and FFI surfaces are
represented in the language's effect vocabulary instead of remaining invisible
compiler side channels.

`quantac check --receipt` also binds each receipt to the checked source inputs
with SHA-256 digests plus compiler and language version metadata. The top-level
`source_digest` records the entry file, `input_digests` records every entry,
import, include, and module file that feeds the check pipeline, and
`input_graph_digest` gives CI a portable fingerprint for the exact source graph
that passed or failed the capability gate.
`quantac receipt verify receipt.json` re-runs the check input graph and confirms
the saved receipt still matches the current source bytes, compiler/language
identity, graph digest, file-backed policy digest, and any recorded built-in
profile digest. Add `--json` to emit a
`quantalang-receipt-verification/v1` report for CI systems that need
machine-readable pass/fail checks instead of human text.

`quantac check --policy <policy.json>` evaluates a portable
`quantalang-check-policy/v1` profile against declared effects and observed
capabilities. Policy failures make the check fail even when type checking
passes, and receipts record the policy path, policy digest, status, and
structured violations.

`quantac policy list` shows built-in baseline profiles,
`quantac policy list --json` emits a machine-readable catalog with profile
digests, and
`quantac policy print <name> --output policy.json` writes one as normal policy
JSON. The initial profiles are `pure`, `console-only`, `offline`, and
`ci-review`, which gives CI a practical starting point before a team adds
function-specific direct and propagated allowlists.

For the common case, `quantac check app.quanta --profile ci-review --receipt -`
evaluates a built-in profile directly without first writing a policy file.
Receipts identify these gates with a `policy.source` such as `builtin:ci-review`.
Built-in profile receipts also include `policy.profile` and
`policy.profile_digest`, so CI can distinguish official profile identity from
an equivalent file-backed policy document.
Use `--expect-profile-digest <hex>` with `--profile` to pin CI to the digest
reported by `quantac policy list --json` or by a prior trusted receipt.

Receipts separate direct capability boundaries from callers that inherit those
effects. `observed_capabilities` records ambient helper, macro, and FFI access
inside a function, such as `read_file`, `tcp_connect`, `println!`, or `touch`.
`propagated_effects` records effectful callees that made a caller inherit a
typed effect. This lets teams permit a small audited boundary function while
still proving which higher-level workflows depend on it.

Policy profiles can enforce that split:

```json
{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["FileSystem", "Network"],
  "direct_effect_allowlist": {
    "FileSystem": ["load_config"]
  },
  "propagated_effect_allowlist": {
    "FileSystem": ["main"]
  },
  "require_source_digest": true,
  "require_input_graph_digest": true
}
```

### Backend Selection

Use `--target` to select a code generation backend:

| Target   | Flag                          | Output  | Status       |
|----------|-------------------------------|---------|--------------|
| C        | `--target c` (default)        | `.c`    | Working      |
| HLSL     | `--target hlsl`               | `.hlsl` | Working      |
| GLSL     | `--target glsl`               | `.glsl` | Working      |
| SPIR-V   | `--target spirv`              | `.spv`  | Experimental |
| LLVM IR  | `--target llvm`               | `.ll`   | Experimental |
| WASM     | `--target wasm`               | `.wasm` | Experimental |
| Rust     | `--target rust` / `--target rs` | `.rs`   | Experimental |
| x86-64   | `--target x86-64`             | `.o`    | Experimental |
| ARM64    | `--target arm64`              | `.o`    | Experimental |

The Rust target emits source for a subset of MIR and is validated with
`rustc --emit=metadata` plus a small executable stdout smoke corpus. The
semantic corpus manifest now drives a Rust execution test, so corpus paths,
expected stdout, generated Rust, `rustc`, and executable behavior are checked
together; manifest contract, receipt consistency, and metadata tests keep the
corpus and Rust execution receipt aligned. The C backend now has a matching
semantic-corpus execution receipt for the same 8 programs, and
`quantac corpus verify` checks the manifest, C/Rust receipts, and real
C-backend stdout from `quantac run`. `quantac corpus verify --root <DIR>`
points verification at a copied corpus, while `--write` rewrites the C
execution receipt after C stdout passes and Rust receipt alignment is still
clean. It currently covers scalar functions, locals, arithmetic, printing,
simple branching, basic structs/arrays/references, tuple ownership reuse,
struct aggregate reuse, field assignment reuse, nested field reuse, and
dereference reuse; unsupported MIR returns a codegen error rather than silent
fallback.

## Status

**132/132 test programs compile.** Full pipeline: `.quanta` -> C99 -> MSVC -> native x86-64 executable. See [TEST_RESULTS.md](TEST_RESULTS.md) for outputs.

Programs cover: functions, recursion, structs, enums, closures, generics, traits, dynamic dispatch, algebraic effects, pattern matching, iterators, hashmaps, vector math, color science, and self-hosted compiler components.

The C backend is the primary target. HLSL/GLSL produce clean shader output. SPIR-V, LLVM, WASM, Rust, x86-64, and ARM64 backends are experimental.

## Design

See [DESIGN.md](DESIGN.md) for full architectural documentation including:
- Pipeline overview (lexer -> parser -> types -> MIR -> backends)
- Type system rationale: why bidirectional inference, why Pratt parsing, why setjmp/longjmp for effects
- MIR design: SSA with basic blocks, statement/terminator model
- Known limitations: borrow/lifetime checking is still early, Rust-target validation is subset-only, eager monomorphization, one-shot effects

## Code Quality

- **CI**: clippy (correctness) + rustfmt + `cargo test` on Linux and Windows
- **Warning gate**: local `RUSTFLAGS=-Dwarnings cargo test --manifest-path compiler/Cargo.toml --quiet` is clean as of 2026-06-14
- **Error handling**: Parser uses `expect()` with messages, lexer has 30+ error variants for recovery, pkg layer uses full `Result<T, E>` propagation
- **Codegen unwraps**: Intentional assertions on validated AST (documented policy in `codegen/mod.rs`)
- **Tests**: 715 passing, 0 failing, 10 ignored, 4 filtered in local `cargo test -- --skip spirv::tests::test_triangle --skip spirv::tests::test_write` from `compiler/` on 2026-06-14
  - Type inference: 54 tests (unification, bidirectional flow, effect inference, const generics)
  - Lexer: 51 tests (token types, spans, Unicode, edge cases, error recovery)
  - Parser: 85 tests (all expression/item/pattern forms, malformed programs)
  - CLI: binary-level smoke tests cover help output, `quantac doctor`, `quantac corpus verify`, `quantac receipt verify`, explicit corpus roots, C receipt writes against copied corpus fixtures, capability diagnostics, and the runnable quickstart examples
  - Codegen: tests across 9 backends, including C formatted-print lowering, Rust source emission, Rust executable smoke checks over the semantic corpus, and semantic-corpus manifest contract/receipt consistency/metadata guards (C backend has 24 end-to-end output verification tests)

## License

MIT License. See [LICENSE](LICENSE) for details.
