# QuantaLang Project Status

Last audited: 2026-06-15

## Identity
The Effects Language -- algebraic effects as a first-class feature.

## What Works (verified, tested, compiles)
- **Lexer**: Complete Unicode-aware tokenizer with comprehensive token types, spans, error recovery. 59 unit tests + 51 integration tests.
- **Parser**: Full recursive descent with Pratt parsing for expressions. Handles functions, structs, enums, match, if/else, loops, effects, generics, patterns. 4 unit tests + 85 integration tests.
- **Type Checker**: Hindley-Milner inference, effect tracking, unification, trait resolution, const generics, higher-kinded types. Function and method signatures now carry declared capability effects, and function effect rows participate in unification so effectful callbacks cannot be accepted by pure `fn(...)` boundaries. Callbacks such as `fn() with FileSystem`, inherent methods such as `Config.load`, associated functions such as `Config::load`, and trait-object methods such as `Loader.load` propagate typed accountability through higher-order calls, static method-call syntax, associated-function syntax, and dynamic dispatch, including effectful closure values, pure tuple-struct and tuple-/struct-like enum-variant callback storage, anonymous immediate closure calls, awaited async block effects with latent origin receipts such as `task <- read_file`, selected async future branch-effect unions for `if` and `match`, caller-side callback argument evidence such as `run(load_config)`, tuple-, tuple-struct-, struct-, enum-variant-, slice-, branch-local `if let`/`while let`, `if`/`if let`/`match` selected callback, cast selected callback, and reference/dereference selected callback evidence, pure callback casts checked against function effect rows, pipe application checked as effectful function application, ordinary binary operators rejecting function values instead of pretending to compose callbacks, invalid `?` and `.await` use rejected on plain callback values before it can erase effect rows, refreshed provenance for assigned callback aliases, aggregate-member callback slots, whole-aggregate callback assignments, nested-block mutations of outer callback aliases and aggregate slots, conservative source merging for conditional, if let, match, explicit loop break, while, while let, and for assignment into outer callback aliases and aggregate slots, nested aggregate-member provenance, nested `if let` selected aggregate field provenance, destructured nested aggregate provenance, direct struct-update-expression destructuring provenance, explicit struct-update field replacement, aggregate-literal destructuring, control-flow-selected aggregate provenance pruning, shorthand aggregate-field provenance pruning, stored enum-variant aggregate payload provenance pruning, and same-scope/inner-scope shadowed opaque aggregate provenance pruning, repeated indexed callback arrays, and nested struct-update-inherited callback fields, compile-time include/environment macros gated as direct `FileSystem`/`Environment` sources, macro argument token trees scanned with `SourceId` provenance so ambient helpers, macros, unknown extern calls, and foreign statics cannot hide inside macro invocations or external module files, known effectful `quanta_*` C runtime aliases declared in extern blocks classified under their domain capability instead of generic `Foreign`, and foreign static reads surfaced as direct `Foreign` boundaries. **Interprocedural lifetime analysis**: lifetime parameters in function types (`FnTy`), lifetime-aware call-site borrow tracking, return lifetime validation via unification. Unit tests across multiple files.
- **C Backend**: Generates valid C99 from QuantaLang source. Handles structs, unions, globals, string tables, branching, all binary/unary ops. 11 unit tests. This is the only backend with end-to-end native execution verified by the compiler test suite.
- **Effects**: Parse -> type check -> codegen pipeline (setjmp/longjmp C runtime).
- **Programs that compile**: Variables, functions, if/else, loops, match, recursion, arithmetic, effects -- all compile to C and execute via `quantac build`.
- **Auto-compile**: `quantac build` discovers and invokes system C compiler (gcc/clang/MSVC).
- **CLI subcommands**: `lex`, `parse`, `check`, `build`, `run`, `test`, `repl`, `version`, `doctor`, `corpus`, `policy`, `receipt`, `lsp`, `fmt`, `pkg`, `watch`.
- **MIR pipeline**: Full MIR builder (codegen/builder.rs, 29 tests), MIR IR (codegen/ir.rs, 31 tests), debug info (codegen/debug.rs, 24 tests), embedded C runtime (codegen/runtime.rs, 7 tests).
- **Macro expansion**: Builtin macros, pattern matching, hygiene. Unit tests present.
- **Interprocedural Lifetime Analysis** (Phase 1): Lifetime parameters flow through `FnTy` (function types), enabling precise borrow tracking at call sites. Functions like `fn pick<'a, 'b>(x: &'a i32, y: &'b i32) -> &'a i32` correctly propagate only the `'a`-linked borrow. Return lifetime mismatches (returning `'b` where `'a` expected) are rejected with clear errors. 8 new unit tests, 3 integration test programs.
- **Current CI-shaped cargo baseline (2026-06-15)**: 868 passed, 0 failed, 10 ignored, 4 filtered across the compiler test binaries via `cargo test -- --skip spirv::tests::test_triangle --skip spirv::tests::test_write` from `compiler/`.
- **Warning-clean baseline (2026-06-15)**: the same 868-test suite passes with `RUSTFLAGS=-Dwarnings`, so release builds are not carrying compiler-warning debt.
- **Wind-down assessment (2026-06-15)**: see `docs/COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md`. Broad language/backend expansion should pause; C remains the product anchor, Rust remains an experimental validation lane, and x86/x64 remains preserved backend research.

## What's Partial (has real code, wired into CLI but not end-to-end verified)
- **Rust Backend** (subset-based): Emits Rust source from MIR and is wired into the CLI via `quantac build --target rust` / `--target rs`. Generated Rust is validated with `rustc --emit=metadata` for 14 subset tests covering scalar branching, references, structs/arrays, struct-field references, repeated non-`Copy` struct arrays, reused structs after assignment and by-value calls, reused tuple values after by-value calls, reused non-`Copy` values after field assignment, reused non-`Copy` struct and tuple aggregate fields, reused non-`Copy` nested field access, reused non-`Copy` dereference, and a lifetime smoke program. A narrower semantic-corpus execution slice compiles generated Rust to executables and asserts stdout for 8 programs: scalar branching, reference mutation, structs/arrays, tuple ownership reuse, struct aggregate reuse, field assignment reuse, nested field reuse, and dereference reuse. The semantic corpus manifest also drives a Rust execution test so manifest paths, expected stdout, backend lowering, and executable behavior stay coupled; manifest contract, receipt consistency, and metadata tests keep the manifest and Rust execution receipt aligned. Unsupported MIR returns a codegen error rather than silent fallback.
- **x86-64 Backend** (1,716 lines, 22 tests): Generates assembly from MIR. Wired into CLI via `quantac build --target x86-64`. No linker integration yet - outputs .s assembly.
- **ARM64 Backend** (1,718 lines, 21 tests): Generates assembly from MIR. Wired into CLI via `quantac build --target arm64`. No linker integration yet - outputs .s assembly.
- **WASM Backend** (2,215 lines, 11 tests): Generates WebAssembly binary from MIR with WASI support. Wired into CLI via `quantac build --target wasm`. No end-to-end .wasm execution test.
- **LLVM Backend** (3,041 lines, 11 tests): Generates LLVM IR text from MIR. Wired into CLI via `quantac build --target llvm`. Optionally compiles to executable with clang. Requires external LLVM tools.
- **SPIR-V Backend** (5,417 lines, 7 tests): Generates SPIR-V binary for Vulkan compute. Wired into CLI via `quantac build --target spirv`. No Vulkan validation test.
- **x86-64 Instruction Encoder** (2,123 lines, 38 tests): Encodes x86-64 instructions to binary machine code. Works in isolation but no linker/loader to produce executables.
- **ARM64 Instruction Encoder** (2,073 lines, 32 tests): Encodes ARM64 instructions to binary. Same limitation.
- **LSP Server** (7,604 lines, 44 tests): Provider implementations exist for completion, hover, compiler-backed diagnostics, go-to-definition, symbols, semantic tokens v0, formatting, folding ranges, code actions, and rename. Wired into CLI via `quantac lsp`; the raw dispatch path now uses structural JSON-RPC parsing and has a semantic-corpus LSP dispatch receipt for lifecycle, document sync, provider requests including `textDocument/semanticTokens/full`, code actions, rename, and compiler-backed diagnostic notifications. End-to-end VS Code extension behavior and full compiler-backed semantic token indexing are not yet receipt-verified.
- **Formatter** (1,657 lines, 11 tests): Code formatter with configurable style. Wired into CLI via `quantac fmt <file>`. Supports `--check` and `--write` flags.
- **Package Manager** (3,503 lines, 24 tests): Manifest parsing (Quanta.toml), semver, lockfile, dependency resolution. Wired into CLI via `quantac pkg`. No registry exists yet.
- **Runtime: FFI** (1,118 lines, 7 tests): Calling convention definitions, type layout, ABI classification. Not used by any code generation backend.
- **Runtime: GC** (789 lines, 4 tests): Reference counting with cycle detection design. Not linked into compiled programs.
- **Runtime: Async** (1,256 lines, 6 tests): Work-stealing scheduler design. Parser and type-checker surfaces exist for async blocks and `.await`, including delayed capability-effect accountability on awaited futures, latent ambient source provenance in await diagnostics/receipts, branch-effect unioning for `if`/`match` selected async futures, and rejection of concrete non-future `.await` operands before they can launder callback effects, but the async runtime is not linked into compiled programs and async execution is not end-to-end verified.

## What's Aspirational (architecture exists, doesn't function)
- **Self-hosted compiler** (quantalang/src/, 217,961 lines): Complete compiler written in QuantaLang (lexer, parser, AST, types, HIR, MIR, codegen for x86_64/AArch64/WASM, driver, LSP, package manager, formatter, linter, test framework, build system, doc generator). **Cannot be compiled or executed.** The Rust compiler does not support the `.quanta` module system, import syntax, or standard library used by this code.
- **Self-hosted stdlib** (quantalang/stdlib/, 26,124 lines): Core library (Option, Result, Iterator, primitives, memory, pointers), Alloc library (Box, Vec, String, Rc), Std library (fs, thread, sync, net, time, process). Modeled after Rust's standard library. **Cannot be compiled or executed.**
- **Self-hosted test suite** (quantalang/tests/, 7,505 lines): Test framework and test cases for the self-hosted compiler. **Cannot be executed.**

## Honest Line Counts
- Compiler source (Rust, `compiler/src/**/*.rs`): 88,946 lines -- STATUS: working core (lexer, parser, types, C backend), partial other backends/tools
- Compiler test sources (Rust, `compiler/tests/`): 10,976 lines -- STATUS: working
- Compiler tree total (tracked Rust under `compiler/`, excluding build output): 100,171 lines across 92 files
- Self-hosted compiler (QuantaLang, `quantalang/src/`): 217,961 lines -- STATUS: aspirational, cannot compile
- Self-hosted stdlib (QuantaLang, `quantalang/stdlib/`): 26,124 lines -- STATUS: aspirational, cannot compile
- Self-hosted tests (QuantaLang, `quantalang/tests/`): 7,505 lines -- STATUS: aspirational, cannot execute

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
quantac doctor              # Diagnose compiler/toolchain/backend readiness
quantac policy list         # List built-in check policy profiles
quantac policy print <name> # Emit a built-in check policy profile as JSON
quantac policy scaffold <receipt.json> # Scaffold an exact strict policy from receipt evidence
quantac receipt verify <receipt.json> [--json]  # Verify a saved check receipt against current source inputs
quantac corpus verify       # Verify semantic corpus receipts and C stdout
quantac corpus verify --root <dir> --write  # Verify a corpus copy and refresh its C receipt
quantac version             # Print version
```

Not yet wired: `doc` subcommand. All other subcommands have CLI entry points.
`quantac test` is a legacy fixture runner, not the current release gate; a live
run on 2026-06-15 starts 137 tests and stops at
`tests/programs/04_if_else.quanta` because older fixtures predate explicit
`Console` capability annotations, reporting 3 passed, 1 error, and 16 skipped
before aborting. `quantac lint` provides type errors + style warnings with
file:line:col positions. `quantac lsp` starts the current stdio server loop, and
the raw dispatch path now covers lifecycle, document sync, completion, hover,
definition, references, symbols, semantic tokens v0, formatting, folding
ranges, code actions, rename, and compiler-backed diagnostics through
structural JSON-RPC parsing.
End-to-end VS Code extension behavior is still not receipt-verified.

## Output Optimization
- **Dead local elimination**: Removes unused MIR temporary declarations
- **Trivial goto elimination**: Removes sequential goto→label pairs from MIR block boundaries
- **Copy propagation**: Framework implemented, needs MIR-level dataflow analysis (disabled)

## Standard Library
Automatic stdlib resolution from any directory via `find_stdlib_path()`. 13 modules (890 lines) in `stdlib/`: core, math, string_utils, algorithms, bitwise, effects, graphics, io, iter, option, result, sorting, strings. Module import call rewriting maps bare function names to prefixed versions.

## Summary
The semantic corpus now also checks a `quantalang-symbol-graph-receipt/v0`
artifact for source/MIR/effect symbol evidence without claiming call graph or
package API completion. LSP readiness is tracked separately through the checked
`quantalang-lsp-dispatch-receipt/v0` artifact and still excludes end-to-end VS
Code extension verification.

QuantaLang has a **working compiler core** (lexer -> parser -> type checker -> MIR -> C backend -> executable) with an 868-passing-test CI-shaped local baseline as of 2026-06-15. It can compile and run real programs with variables, functions, control flow, pattern matching, recursion, and algebraic effects. C, LLVM, x86-64, ARM64, WASM, SPIR-V, HLSL, GLSL, and Rust are accessible from the CLI via `quantac build --target <target>`, but with different maturity levels. The C backend is production-verified and now has a semantic-corpus C execution receipt matching the current 8-program corpus; `quantac run` uses per-run temp build directories so concurrent C receipt probes avoid shared temp C/PDB collisions; `quantac corpus verify` validates the semantic corpus manifest, C/Rust receipts, and real C-backend stdout, accepts explicit corpus roots, and can refresh the C receipt for copied corpus fixtures after C stdout passes. The same corpus path now carries a `quantalang-substrate-receipt/v0` aggregation receipt that checks source-set size, backend maturity, memory gaps, representation fallback policy, and evidence commands without promoting experimental backends. Its representation surface is now backed by a checked `quantalang-mir-representation-receipt/v0` artifact that recomputes per-program MIR operation families, symbols, memory-surface flags, and control-flow summaries during `quantac corpus verify`. The same verification path now also checks a `quantalang-memory-layout-receipt/v0` artifact that binds the corpus memory surface to manifest tags, MIR-derived memory flags, ownership/layout classification, digest evidence, and explicit known gaps without claiming byte-level ABI layout or full borrow proof. `quantac receipt verify` re-checks saved source-bound check receipts against current source inputs, policy/profile digests, replayed effect/accountability surfaces, optional required built-in profile identity, and optional required policy digest, with optional JSON verification reports for CI; check policies now validate referenced effect names against built-in capabilities and the checked source graph so misspelled gates fail instead of silently weakening enforcement, can require `allowed_effects` to be authoritative even when empty, can require explicit direct/propagated provenance allowlists, can constrain direct capability boundaries to exact ambient helper/macro/FFI sources, can classify compile-time ambient macros such as `include_str!` and `env!` under `FileSystem`/`Environment`, can scan macro argument token trees with `SourceId` provenance so `println!(read_file(...))` requires both `Console` and `FileSystem` in entry sources and external module files and unknown extern calls/statics surface as `Foreign`, can classify known effectful `quanta_*` C runtime helper aliases declared in extern blocks under their real domain capability instead of generic `Foreign`, can preserve qualified ambient helper paths such as `io::read_file` in diagnostics, receipts, and scaffolded source allowlists, can reject effectful callbacks passed into pure `fn(...)` boundaries instead of erasing effect rows, and can preserve delayed or propagated capability evidence across callbacks, closures, aggregates, async awaits, branches, loops, casts, refs/derefs, pipes, assignments, selected aggregate fields, returned functions, and exact source allowlists. The built-in `strict-accountability` policy profile packages required effect inventory, digest, provenance, source, and coverage requirements into a named adoption gate for teams that want no ambient IO without exact allowlists, and `quantac policy scaffold` can turn observed receipt evidence into an exact strict policy skeleton for review while preserving pure receipts against later effect drift. `quantac doctor` reports local toolchain, stdlib, registry, optional backend tools, and backend maturity for adoption diagnostics; tested quickstart examples cover first-run CPU execution, mutable control flow, algebraic effects, and HLSL shader output; the Rust backend is subset-validated with `rustc --emit=metadata` and has a narrower generated-executable stdout smoke layer over the same semantic corpus plus manifest contract/receipt consistency/metadata guards; LLVM can optionally link with clang; native/WASM backends output assembly/binary for external toolchain linking. Formatter and package-manager entry points (`quantac fmt`, `quantac pkg`) are wired into the CLI, but the package manager has no live registry. `quantac lsp` starts the current stdio server loop, dispatches the checked raw LSP receipt sequence through structural JSON-RPC parsing, emits compiler-backed diagnostics, and returns receipt-verified semantic tokens v0; full compiler-backed semantic token indexing and end-to-end VS Code extension verification remain open. The self-hosted compiler and standard library (244,085 lines of `.quanta` code) represent an ambitious long-term vision but cannot be compiled or executed today.
