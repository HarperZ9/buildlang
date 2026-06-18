# QuantaLang Usage Guide

This guide covers installing the `quantac` compiler and using its real
command surface. QuantaLang compiles `.quanta` source files to **C** as the
primary verified execution path and emits **HLSL**/**GLSL** for shader work,
with additional experimental backends.

All commands and flags below are taken from the compiler's actual CLI
definition (`compiler/src/main.rs`). The worked examples were run against a
local debug build of `quantac` 1.0.0 on Windows; the captured output is shown
verbatim. Output captured from an actual run is marked **(verified)**; any
output that was not run is marked **(illustrative)**.

## Install / Build

Build the compiler from source with Cargo:

```bash
cd compiler
cargo build --release
```

Add the produced binary to your `PATH`:

- Linux/macOS: `compiler/target/release/quantac`
- Windows: `compiler\target\release\quantac.exe`

Confirm your local toolchain (C compiler, stdlib, optional backend tools):

```bash
quantac doctor
```

## Command Reference

These are the subcommands exposed by `quantac` (run `quantac --help` for the
authoritative list and `quantac <command> --help` for per-command flags):

| Command            | Purpose                                                      |
|--------------------|-------------------------------------------------------------|
| `quantac <file>`   | Compile a file (no subcommand); honors `-o`, `--target`, `-O`, `-g` |
| `quantac run`      | Compile and run a `.quanta` file via the C backend          |
| `quantac build`    | Build a project (`--emit c|exe`, `--release`, `--target`, `--keep-c`) |
| `quantac check`    | Type-check; optional `--receipt`, `--policy`, `--profile`   |
| `quantac lex`      | Tokenize a file and print tokens                            |
| `quantac parse`    | Parse a file and print the AST (`--json` for JSON)          |
| `quantac fmt`      | Format source (`--check`, `--write`)                        |
| `quantac lint`     | Lint a source file                                          |
| `quantac repl`     | Start a REPL session                                        |
| `quantac lsp`      | Start the Language Server Protocol server                  |
| `quantac watch`    | Watch files and recompile on change (`--target spirv|c`)    |
| `quantac pkg`      | Package manager (`init`, `add`, `resolve`, `search`)        |
| `quantac policy`   | Built-in check policy profiles (`list`, `print`, `scaffold`)|
| `quantac receipt`  | Verify a saved check receipt (`verify`)                     |
| `quantac corpus`   | Verify the semantic corpus (`verify`)                       |
| `quantac test`     | Run `.quanta` programs against `.expected` files            |
| `quantac doctor`   | Diagnose local toolchain, backend, and package readiness    |
| `quantac version`  | Print version information                                   |

### Top-level compile flags

When invoked without a subcommand (`quantac <file>`):

- `-o, --output <FILE>` — output file path
- `--target <NAME>` — code generation backend (see below)
- `-O, --opt-level <0-3>` — optimization level (default `0`)
- `-g, --debug` — emit debug information
- `-v, --verbose` — verbose output

### Code generation targets

`--target` (and `quantac build --target`) accepts:

| Target  | Flag value(s)              | Status       |
|---------|----------------------------|--------------|
| C       | `c` (default)              | Primary      |
| HLSL    | `hlsl`, `dx`, `directx`    | Supported    |
| GLSL    | `glsl`, `opengl`, `gl`     | Supported    |
| Rust    | `rust`, `rs`               | Experimental |
| LLVM IR | `llvm`, `llvm-ir`, `ll`    | Experimental |
| WASM    | `wasm`, `wasm32`, `wat`    | Experimental |
| SPIR-V  | `spirv`, `spir-v`, `spv`   | Experimental |
| x86-64  | `x86-64`, `x86_64`, `x64`  | Experimental |
| ARM64   | `arm64`, `aarch64`         | Experimental |

## Worked Examples

The repository ships tested quickstart programs under `examples/quickstart/`.
The examples below use those files so they stay aligned with the compiler.

### 1. Run a program

`examples/quickstart/hello.quanta`:

```quanta
fn main() ~ Console {
    println!("Hello from QuantaLang!");
}
```

`println!` is a `Console` capability, so `main` declares the `~ Console`
effect. Compile and run via the C backend:

```bash
quantac run examples/quickstart/hello.quanta
```

Output **(verified)**:

```
Hello from QuantaLang!
```

The `ledger.quanta` example (functions, mutable locals, a `while` loop):

```bash
quantac run examples/quickstart/ledger.quanta
```

Output **(verified)** — `100 + 5*3`:

```
balance: 115
```

### 2. Compile to C and build by hand

Emit C and compile it with your system C compiler:

```bash
quantac examples/quickstart/hello.quanta -o hello.c
cc hello.c -o hello
./hello
```

The first command prints (path will be your output path) **(verified)**:

```
Compiled examples/quickstart/hello.quanta -> hello.c
```

The generated C begins with `// Generated by QuantaLang Compiler` and a
portability prelude before the lowered program.

### 3. Type-check with a capability policy and receipt

`quantac check` type-checks and reports capability effects. Add `--profile`
to evaluate a built-in policy and `--receipt -` to print a machine-readable
accountability receipt to stdout:

```bash
quantac check examples/quickstart/hello.quanta --profile console-only --receipt -
```

The human summary is printed first, then the JSON receipt. Output begins
**(verified)**:

```
Lexing... OK (15 tokens)
Parsing... OK (1 items)
Type checking... OK

No errors found in 'examples/quickstart/hello.quanta'
{
  "schema": "quantalang-check-receipt/v1",
  "compiler": "quantac",
  "compiler_version": "1.0.0",
  "language_version": "1.0.0",
  ...
  "status": "passed",
  "declared_effects": {
    "main": [
      "Console"
    ]
  },
  ...
}
```

List the built-in policy profiles with:

```bash
quantac policy list
```

Output **(verified)**:

```
Built-in check policy profiles:
  pure           deny all built-in ambient capability effects
  console-only   allow Console only; deny other ambient capability effects
  offline        allow local file/env/clock/console work; deny network/process/FFI/GPU
  ci-review      require digests and deny Network, Process, Foreign, and Gpu
  strict-accountability require digests, exact allowlists, and deny Network/Process/FFI/GPU
```

Save a receipt to a file and re-verify it later against the current source:

```bash
quantac check app.quanta --profile ci-review --receipt receipt.json
quantac receipt verify receipt.json --expect-profile ci-review
```

### 4. Compile a shader to HLSL

`examples/quickstart/vignette_shader.quanta` defines a `#[fragment]` entry
point. Compile it to HLSL for ReShade / DirectX:

```bash
quantac examples/quickstart/vignette_shader.quanta --target hlsl -o vignette_shader.hlsl
```

The command prints **(verified)**:

```
Compiled examples/quickstart/vignette_shader.quanta -> vignette_shader.hlsl
```

The generated HLSL **(verified, excerpt)**:

```hlsl
// Generated by QuantaLang Compiler
// Target: HLSL (DirectX / ReShade)
// Do not edit manually

float vignette(float uv_x, float uv_y, float strength, float softness) {
    float dx = (uv_x - 0.5);
    float dy = (uv_y - 0.5);
    float dist = sqrt(((dx * dx) + (dy * dy)));
    float vig = smoothstep(0.5, (0.5 * softness), dist);
    return (1.0 - (strength * (1.0 - vig)));
}

float4 PS_Vignette(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target0 {
    float4 color = tex2D(ReShade::BackBuffer, uv);
    float vig = vignette(uv.x, uv.y, 0.5, 0.6);
    return float4((color.x * vig), (color.y * vig), (color.z * vig), 1.0);
}
```

Use `--target glsl` to emit GLSL for OpenGL / Vulkan instead.

## More

- A runnable demo: [examples/demo](examples/demo)
- Quickstart programs: [examples/quickstart](examples/quickstart)
- Getting started tutorial: [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)
- Capability effects reference: [docs/EFFECTS_GUIDE.md](docs/EFFECTS_GUIDE.md)
- Shader output reference: [docs/SHADER_GUIDE.md](docs/SHADER_GUIDE.md)
- Architecture and design: [DESIGN.md](DESIGN.md)
