# BuildLang Architecture

## Overview

BuildLang -- "The Effects Language" -- has a working Rust-based compiler and a large body
of aspirational self-hosted code. This document explains what each directory contains and
what actually works today.

## `compiler/` -- The Real Compiler (Rust)

This is the working implementation. It compiles BuildLang source to C99, invokes a system
C compiler (gcc/clang/MSVC), and produces native executables.

**Pipeline:** Lexer -> Parser -> Type Checker -> MIR -> C Backend -> Executable

What works end-to-end:
- Variables, functions, control flow (if/else, while, match)
- Structs, enums, pattern matching, recursion
- Algebraic effects (define, perform, handle) via setjmp/longjmp C runtime
- 868-test CI-shaped local baseline as of 2026-06-15: 666 library tests,
  10 doctest/auxiliary tests, 192 CLI tests, and 10 ignored tests, with
  4 SPIR-V tests filtered in the documented command

What exists with narrower maturity:
- Rust, LLVM, WASM, SPIR-V, x86-64, and ARM64 backends are selectable through
  `buildc build --target`, but remain experimental or validation lanes rather
  than production targets.
- HLSL and GLSL shader output are useful public adjuncts, with less release
  weight than the C backend.
- LSP server, code formatter, and package manager have CLI entry points
  (`buildc lsp`, `buildc fmt`, `buildc pkg`), but LSP request dispatch is
  still limited and the package manager has no live registry.

Key paths:
- `compiler/src/lexer/` -- tokenizer
- `compiler/src/parser/` -- recursive descent + Pratt parsing
- `compiler/src/types/` -- Hindley-Milner inference, effect tracking
- `compiler/src/codegen/` -- MIR builder, C backend, native backends
- `compiler/src/lsp/` -- language server
- `compiler/src/fmt/` -- code formatter
- `compiler/src/pkg/` -- package manager
- `compiler/tests/` -- integration tests

CLI today:
```
buildc lex <file>       # Tokenize and print tokens
buildc parse <file>     # Parse and print AST
buildc check <file>     # Type-check
buildc build [path]     # Compile to C -> native executable
buildc build --target <target>
buildc run <file>       # Compile and run
buildc test             # Legacy fixture runner; not the current release gate
buildc fmt <file>       # Format source
buildc pkg <subcommand> # Package manager surface
buildc watch [path]     # Watch and recompile
buildc lsp              # Start current LSP server loop
buildc doctor           # Toolchain/backend readiness diagnostics
buildc corpus verify    # Verify semantic corpus receipts and C stdout
buildc policy ...       # Built-in check policy profiles
buildc receipt verify   # Verify saved check receipts
buildc repl             # Interactive REPL
buildc version          # Print version
```

## `future/` -- Aspirational Self-Hosted Compiler

**The self-hosted compiler in `future/` is a design document expressed as code. It
represents the future vision but cannot be compiled by the current compiler.**

This directory contains 251,590 lines of `.bld` code: a complete self-hosted compiler,
standard library, and test suite. None of it compiles or executes. The Rust compiler does
not yet support the module system, import syntax, generics, or standard library that this
code relies on.

Key paths:
- `future/self-hosted-compiler/` -- self-hosted compiler (lexer, parser, AST, HIR, MIR,
  codegen for x86_64/AArch64/WASM, driver, LSP, package manager, formatter, linter,
  doc generator, test framework, build system)
- `future/stdlib/` -- standard library (core, alloc, std -- modeled after Rust's stdlib)
- `future/tests/` -- test suite for the self-hosted compiler
- `future/release/` -- release packaging

This code is valuable as a specification and roadmap. It defines what BuildLang's syntax,
standard library, and tooling should look like when the language is capable of self-hosting.

## `buildlang/` -- Aspirational Self-Hosted Tree and Historical Release Materials

What remains in this directory:
- `examples/` -- aspirational example programs (effects demos, HTTP server,
  CLI tool, concurrency)
- `docs/` -- aspirational language guides, release docs, and API docs
- `scripts/` -- installer
- `website/` -- historical website mockup, not the current public status page
- `STATUS.md`, `ASPIRATIONAL.md` -- status tracking

Treat this tree as future-facing source material unless a file explicitly says
it is verified by the Rust compiler in `compiler/`.

## `tests/` -- Legacy and Targeted Test Programs

This directory contains legacy fixture programs, shader fixtures, cross-target
artifacts, and focused examples. Some files still compile and run with the
current Rust compiler, but the historical `buildc test` sweep is no longer a
green release gate because older fixtures predate explicit capability
annotations.

The active release-shaped proof is the Cargo baseline in `README.md` and
`STATUS.md`, plus semantic-corpus receipt verification. A live `buildc test`
run on 2026-06-15 starts 137 legacy fixtures and stops at
`tests/programs/04_if_else.bld` because that fixture lacks the now-required
`Console` capability annotation.

## `src/` -- Top-Level Source

Contains additional BuildLang source files (lexer, parser, stdlib, VM) at the project
root level. Like `buildlang/buildlang/`, these are aspirational and do not compile with
the current compiler.

## Line Counts

| Directory | Lines | Language | Status |
|---|---|---|---|
| `compiler/src/` | 88,946 | Rust | Working core, partial tools |
| `compiler/tests/` | 10,976 | Rust | Working CLI/integration tests |
| `tests/programs/` | 153 `.bld` files | BuildLang | Legacy/current mixed fixtures |
| `future/self-hosted-compiler/` | 217,961 | BuildLang | Aspirational |
| `future/stdlib/` | 26,124 | BuildLang | Aspirational |
| `future/tests/` | 7,505 | BuildLang | Aspirational |
