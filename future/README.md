# Future: Self-Hosted BuildLang

This directory contains the aspirational self-hosted BuildLang implementation.
**This code cannot be compiled by the current compiler.**

When BuildLang gains module imports, trait dispatch, and a standard library,
this code will become the target for self-hosting - a compiler that compiles itself.

## Contents
- `self-hosted-compiler/` - Compiler written in BuildLang
- `stdlib/` - Standard library (core, alloc, std)
- `tests/` - Test suite for the self-hosted compiler
- `release/` - Release packaging scripts

## Status
See the main project's STATUS.md for what the Rust compiler currently supports.
