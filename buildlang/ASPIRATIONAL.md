# Self-Hosted BuildLang - Aspirational Compilation Target

Current status (2026-06-15): this directory is aspirational architecture and
historical release scaffolding. **None of this self-hosted compiler, standard
library, or toolchain tree can be compiled or executed by the current
Rust-based compiler.**

It serves as the **compilation target** - as the Rust compiler gains features (trait dispatch,
module imports, standard library), more of this code becomes compilable. The goal is
self-hosting: a BuildLang compiler that compiles itself.

## Current Compilability

The current Rust compiler status lives in the repository root `README.md` and
`STATUS.md`. It has a working C-backed compiler core and semantic corpus
receipts, but it does not support the module system, import syntax, or standard
library assumptions used by this self-hosted tree.

Files in this directory should be treated as design targets until an explicit
bootstrap/compilation receipt proves otherwise.

## Contents
- `src/` - Self-hosted compiler (lexer, parser, AST, type checker, codegen)
- `stdlib/` - Standard library (core, alloc, std modules)
- `tests/` - Test suite for the self-hosted compiler
- `examples/` - Example programs including effect demonstrations
- `docs/` - Language documentation and manifesto

## Why Keep It Here?
This code is not dead weight - it's the specification for what BuildLang should become.
As the Rust compiler gains features, we progressively compile more of these files,
working toward the ultimate goal: self-hosting.
