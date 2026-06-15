# QuantaLang Architecture

## Overview

QuantaLang -- "The Effects Language" -- has a working Rust-based compiler and a large body
of aspirational self-hosted code. This document explains what each directory contains and
what actually works today.

## `compiler/` -- The Real Compiler (Rust)

This is the working implementation. It compiles QuantaLang source to C99, invokes a system
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
  `quantac build --target`, but remain experimental or validation lanes rather
  than production targets.
- HLSL and GLSL shader output are useful public adjuncts, with less release
  weight than the C backend.
- LSP server, code formatter, and package manager have CLI entry points
  (`quantac lsp`, `quantac fmt`, `quantac pkg`), but LSP request dispatch is
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
quantac lex <file>       # Tokenize and print tokens
quantac parse <file>     # Parse and print AST
quantac check <file>     # Type-check
quantac build [path]     # Compile to C -> native executable
quantac build --target <target>
quantac run <file>       # Compile and run
quantac test             # Legacy fixture runner; not the current release gate
quantac fmt <file>       # Format source
quantac pkg <subcommand> # Package manager surface
quantac watch [path]     # Watch and recompile
quantac lsp              # Start current LSP server loop
quantac doctor           # Toolchain/backend readiness diagnostics
quantac corpus verify    # Verify semantic corpus receipts and C stdout
quantac policy ...       # Built-in check policy profiles
quantac receipt verify   # Verify saved check receipts
quantac repl             # Interactive REPL
quantac version          # Print version
```

## `future/` -- Aspirational Self-Hosted Compiler

**The self-hosted compiler in `future/` is a design document expressed as code. It
represents the future vision but cannot be compiled by the current compiler.**

This directory contains 251,590 lines of `.quanta` code: a complete self-hosted compiler,
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

This code is valuable as a specification and roadmap. It defines what QuantaLang's syntax,
standard library, and tooling should look like when the language is capable of self-hosting.

## `quantalang/` -- Aspirational Self-Hosted Tree and Historical Release Materials

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
current Rust compiler, but the historical `quantac test` sweep is no longer a
green release gate because older fixtures predate explicit capability
annotations.

The active release-shaped proof is the Cargo baseline in `README.md` and
`STATUS.md`, plus semantic-corpus receipt verification. A live `quantac test`
run on 2026-06-15 starts 137 legacy fixtures and stops at
`tests/programs/04_if_else.quanta` because that fixture lacks the now-required
`Console` capability annotation.

## `src/` -- Top-Level Source

Contains additional QuantaLang source files (lexer, parser, stdlib, VM) at the project
root level. Like `quantalang/quantalang/`, these are aspirational and do not compile with
the current compiler.

## Line Counts

| Directory | Lines | Language | Status |
|---|---|---|---|
| `compiler/src/` | 88,946 | Rust | Working core, partial tools |
| `compiler/tests/` | 10,976 | Rust | Working CLI/integration tests |
| `tests/programs/` | 153 `.quanta` files | QuantaLang | Legacy/current mixed fixtures |
| `future/self-hosted-compiler/` | 217,961 | QuantaLang | Aspirational |
| `future/stdlib/` | 26,124 | QuantaLang | Aspirational |
| `future/tests/` | 7,505 | QuantaLang | Aspirational |
