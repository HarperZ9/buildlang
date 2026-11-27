# BuildLang for Visual Studio Code

Language support for [BuildLang](https://github.com/HarperZ9/buildlang): syntax highlighting, snippets, file icons, bracket matching, and `buildc lsp` launch for `.bld` files.

## Features

- **Syntax highlighting**: keywords, types, effects, attributes, strings, numbers, comments, macros, and operators.
- **Optional LSP process launch** via `buildc lsp`: starts the compiler language server; diagnostics, go-to-definition, hover, and completion providers are under active expansion.
- **Bracket matching**, auto-closing pairs, and code folding.
- **19 snippets** for common constructs (functions, structs, enums, traits, loops, tests, shaders, effects).
- **Shader attribute highlighting** for `#[vertex]`, `#[fragment]`, and `#[compute]`.
- **Effect system highlighting**: `effect`, `perform`, `handle`, `with`, `resume`, and the `~` effect annotation.

## Install BuildLang (`buildc`, `qdb`, `port`)

```powershell
# Windows (PowerShell, SHA-256 verified)
irm https://raw.githubusercontent.com/HarperZ9/buildlang/main/install.ps1 | iex
```

```bash
# Linux and macOS (SHA-256 verified)
curl -fsSL https://raw.githubusercontent.com/HarperZ9/buildlang/main/install.sh | sh

# Or build buildc from crates.io
cargo install buildlang
```

Then ensure `buildc` is on your `PATH` or set `buildlang.serverPath` in VS Code settings.

## Extension Packaging and Development

```bash
cd editors/vscode
npm ci
npm run compile
npx --yes @vscode/vsce package --no-dependencies
code --install-extension buildlang-1.15.0.vsix
```

Or open `editors/vscode` in VS Code and press **F5** to launch an Extension Development Host.

## Configuration

| Setting | Default | Description |
| --- | --- | --- |
| `buildlang.serverPath` | `buildc` | Path to the `buildc` compiler binary |
| `buildlang.target` | `c` | Default compilation target (`c`, `hlsl`, `glsl`, `wgsl`, `msl`, `spirv`, `llvm`, `wasm`, `x86-64`, `arm64`, `riscv64`, `rust`) |

## Snippets

| Prefix | Description |
| --- | --- |
| `fn` | Function definition |
| `fnm` | Method with `&self` |
| `st` | Struct definition |
| `impl` | Implementation block |
| `enum` | Enum definition |
| `trait` | Trait definition |
| `for` | For loop over range |
| `match` | Match expression |
| `while` | While loop |
| `let` | Let binding |
| `letm` | Mutable let binding |
| `test` | Test function with `#[test]` |
| `mod` | Module definition |
| `vec` | `vec!` macro |
| `println` | `println!` macro |
| `effect` | Effect definition |
| `fragment` | Fragment shader entry point |
| `vertex` | Vertex shader entry point |
| `uniform` | Shader uniform constant |
