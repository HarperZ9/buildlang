# QuantaLang Project Status

Last audited: 2026-06-13

## Identity
The Effects Language -- algebraic effects as a first-class feature.

## What Works (verified, tested, compiles)
- **Lexer**: Complete Unicode-aware tokenizer with comprehensive token types, spans, error recovery. 59 unit tests + 51 integration tests.
- **Parser**: Full recursive descent with Pratt parsing for expressions. Handles functions, structs, enums, match, if/else, loops, effects, generics, patterns. 4 unit tests + 85 integration tests.
- **Type Checker**: Hindley-Milner inference, effect tracking, unification, trait resolution, const generics, higher-kinded types. **Interprocedural lifetime analysis**: lifetime parameters in function types (`FnTy`), lifetime-aware call-site borrow tracking, return lifetime validation via unification. Unit tests across multiple files.
- **C Backend**: Generates valid C99 from QuantaLang source. Handles structs, unions, globals, string tables, branching, all binary/unary ops. 11 unit tests. This is the only backend with end-to-end native execution verified by the compiler test suite.
- **Effects**: Parse -> type check -> codegen pipeline (setjmp/longjmp C runtime).
- **Programs that compile**: Variables, functions, if/else, loops, match, recursion, arithmetic, effects -- all compile to C and execute via `quantac build`.
- **Auto-compile**: `quantac build` discovers and invokes system C compiler (gcc/clang/MSVC).
- **CLI subcommands**: `lex`, `parse`, `check`, `build`, `run`, `test`, `repl`, `version`, `lsp`, `fmt`, `pkg`, `watch`.
- **MIR pipeline**: Full MIR builder (codegen/builder.rs, 29 tests), MIR IR (codegen/ir.rs, 31 tests), debug info (codegen/debug.rs, 24 tests), embedded C runtime (codegen/runtime.rs, 7 tests).
- **Macro expansion**: Builtin macros, pattern matching, hygiene. Unit tests present.
- **Interprocedural Lifetime Analysis** (Phase 1): Lifetime parameters flow through `FnTy` (function types), enabling precise borrow tracking at call sites. Functions like `fn pick<'a, 'b>(x: &'a i32, y: &'b i32) -> &'a i32` correctly propagate only the `'a`-linked borrow. Return lifetime mismatches (returning `'b` where `'a` expected) are rejected with clear errors. 8 new unit tests, 3 integration test programs.
- **Current cargo baseline (2026-06-13)**: 641 passed, 0 failed, 11 ignored across the compiler test binaries via `cargo test --manifest-path compiler/Cargo.toml --quiet`.

## What's Partial (has real code, wired into CLI but not end-to-end verified)
- **Rust Backend** (subset-based): Emits Rust source from MIR and is wired into the CLI via `quantac build --target rust` / `--target rs`. Generated Rust is validated with `rustc --emit=metadata` for 14 subset tests covering scalar branching, references, structs/arrays, struct-field references, repeated non-`Copy` struct arrays, reused structs after assignment and by-value calls, reused tuple values after by-value calls, reused non-`Copy` values after field assignment, reused non-`Copy` struct and tuple aggregate fields, reused non-`Copy` nested field access, reused non-`Copy` dereference, and a lifetime smoke program. A narrower semantic-corpus execution slice compiles generated Rust to executables and asserts stdout for 7 programs: scalar branching, reference mutation, structs/arrays, tuple ownership reuse, struct aggregate reuse, field assignment reuse, and nested field reuse. The semantic corpus manifest also drives a Rust execution test so manifest paths, expected stdout, backend lowering, and executable behavior stay coupled; receipt consistency and metadata tests keep the manifest and Rust execution receipt aligned. Unsupported MIR returns a codegen error rather than silent fallback.
- **x86-64 Backend** (1615 lines, 22 tests): Generates assembly from MIR. Wired into CLI via `quantac build --target x86-64`. No linker integration yet — outputs .s assembly.
- **ARM64 Backend** (1629 lines, 21 tests): Generates assembly from MIR. Wired into CLI via `quantac build --target arm64`. No linker integration yet — outputs .s assembly.
- **WASM Backend** (1866 lines, 11 tests): Generates WebAssembly binary from MIR with WASI support. Wired into CLI via `quantac build --target wasm`. No end-to-end .wasm execution test.
- **LLVM Backend** (1915 lines, 11 tests): Generates LLVM IR text from MIR. Wired into CLI via `quantac build --target llvm`. Optionally compiles to executable with clang. Requires external LLVM tools.
- **SPIR-V Backend** (1898 lines, 7 tests): Generates SPIR-V binary for Vulkan compute. Wired into CLI via `quantac build --target spirv`. No Vulkan validation test.
- **x86-64 Instruction Encoder** (2058 lines, 38 tests): Encodes x86-64 instructions to binary machine code. Works in isolation but no linker/loader to produce executables.
- **ARM64 Instruction Encoder** (2161 lines, 32 tests): Encodes ARM64 instructions to binary. Same limitation.
- **LSP Server** (6448 lines, 24 tests): Full LSP implementation with completion, hover, diagnostics, go-to-definition, symbols, code actions. Wired into CLI via `quantac lsp`. JSON dispatch uses manual string matching. Only lifecycle messages are dispatched in the server loop — cannot serve a full VS Code session yet.
- **Formatter** (1631 lines, 11 tests): Code formatter with configurable style. Wired into CLI via `quantac fmt <file>`. Supports `--check` and `--write` flags.
- **Package Manager** (3354 lines, 24 tests): Manifest parsing (Quanta.toml), semver, lockfile, dependency resolution. Wired into CLI via `quantac pkg`. No registry exists yet.
- **Runtime: FFI** (1038 lines, 7 tests): Calling convention definitions, type layout, ABI classification. Not used by any code generation backend.
- **Runtime: GC** (786 lines, 4 tests): Reference counting with cycle detection design. Not linked into compiled programs.
- **Runtime: Async** (1216 lines, 6 tests): Work-stealing scheduler design. Not linked into compiled programs. No async/await syntax support.

## What's Aspirational (architecture exists, doesn't function)
- **Self-hosted compiler** (quantalang/src/, 231,688 lines): Complete compiler written in QuantaLang (lexer, parser, AST, types, HIR, MIR, codegen for x86_64/AArch64/WASM, driver, LSP, package manager, formatter, linter, test framework, build system, doc generator). **Cannot be compiled or executed.** The Rust compiler does not support the `.quanta` module system, import syntax, or standard library used by this code.
- **Self-hosted stdlib** (quantalang/stdlib/, 28,649 lines): Core library (Option, Result, Iterator, primitives, memory, pointers), Alloc library (Box, Vec, String, Rc), Std library (fs, thread, sync, net, time, process). Modeled after Rust's standard library. **Cannot be compiled or executed.**
- **Self-hosted test suite** (quantalang/tests/, 8,230 lines): Test framework and test cases for the self-hosted compiler. **Cannot be executed.**

## Honest Line Counts
- Compiler (Rust, `compiler/src/`): 61,695 lines -- STATUS: working core (lexer, parser, types, C backend), partial other backends/tools
- Integration Tests (Rust, `compiler/tests/`): 1,522 lines -- STATUS: working
- Self-hosted compiler (QuantaLang, `quantalang/src/`): 231,688 lines -- STATUS: aspirational, cannot compile
- Self-hosted stdlib (QuantaLang, `quantalang/stdlib/`): 28,649 lines -- STATUS: aspirational, cannot compile
- Self-hosted tests (QuantaLang, `quantalang/tests/`): 8,230 lines -- STATUS: aspirational, cannot execute

## What the CLI Actually Does Today
```
quantac lex <file>          # Tokenize and print tokens
quantac parse <file>        # Parse and print AST
quantac check <file>        # Type-check
quantac build [path]        # Compile to C, invoke C compiler, produce executable
quantac build --target llvm # Compile to LLVM IR (.ll), optionally link with clang
quantac build --target x86-64  # Compile to x86-64 assembly
quantac build --target arm64   # Compile to AArch64 assembly
quantac build --target wasm    # Compile to WebAssembly (.wasm)
quantac build --target spirv   # Compile to SPIR-V binary (.spv)
quantac build --target hlsl    # Compile to HLSL shader
quantac build --target glsl    # Compile to GLSL shader
quantac run <file>          # Compile and run (C backend)
quantac repl                # Interactive REPL
quantac lsp                 # Start Language Server Protocol server
quantac fmt <file>          # Format QuantaLang source code
quantac pkg init            # Initialize Quanta.toml manifest
quantac pkg add <name>      # Add a dependency
quantac pkg resolve         # Resolve dependencies and generate lockfile
quantac pkg search <query>  # Search the package registry
quantac watch [path]        # Watch files and recompile on change
quantac version             # Print version
```

Not yet wired: `doc` subcommand. All other subcommands are functional. `quantac test` runs 126/150 programs (84%). `quantac lint` provides type errors + style warnings with file:line:col positions. `quantac lsp` provides real type checker diagnostics to VS Code.

## Output Optimization
- **Dead local elimination**: Removes unused MIR temporary declarations
- **Trivial goto elimination**: Removes sequential goto→label pairs from MIR block boundaries
- **Copy propagation**: Framework implemented, needs MIR-level dataflow analysis (disabled)

## Standard Library
Automatic stdlib resolution from any directory via `find_stdlib_path()`. 13 modules (842 lines) in `stdlib/`: core, math, string_utils, algorithms, bitwise, effects, graphics, io, iter, option, result, sorting, strings. Module import call rewriting maps bare function names to prefixed versions.

## Summary
QuantaLang has a **working compiler core** (lexer -> parser -> type checker -> MIR -> C backend -> executable) with 641 passing cargo tests and 11 ignored tests as of 2026-06-13. It can compile and run real programs with variables, functions, control flow, pattern matching, recursion, and algebraic effects. C, LLVM, x86-64, ARM64, WASM, SPIR-V, HLSL, GLSL, and Rust are accessible from the CLI via `quantac build --target <target>`, but with different maturity levels. The C backend is production-verified; the Rust backend is subset-validated with `rustc --emit=metadata` and has a narrower generated-executable stdout smoke layer over the semantic corpus plus receipt consistency/metadata guards; LLVM can optionally link with clang; native/WASM backends output assembly/binary for external toolchain linking. The LSP (`quantac lsp`), formatter (`quantac fmt`), and package manager (`quantac pkg`) are wired into the CLI. The self-hosted compiler and standard library (268,567 lines of `.quanta` code) represent an ambitious long-term vision but cannot be compiled or executed today.
