# AGENTS.md - QuantaLang

## Scope

This file applies to the QuantaLang repository. Root workspace instructions
still apply; this repo is a public compiler, language, editor-support, and
semantic-corpus product surface.

## Product Boundary

QuantaLang is the public language/compiler anchor for the Quanta ecosystem.
Treat it as a release-candidate product with several maturity layers:

- Primary: C backend, CLI, parser/typechecker/MIR pipeline, docs, examples, and
  test programs.
- Supported public adjuncts: HLSL/GLSL shader output, VS Code extension sources,
  semantic corpus, language docs, and release packaging docs.
- Experimental: Rust, LLVM, WASM, SPIR-V, x86-64, and ARM64 backends. These must
  state their maturity plainly and fail loudly on unsupported MIR.
- Aspirational: self-hosting and broader `quantalang/` release materials. Keep
  them described as future or partial unless the compiler can build them.

Publishable surfaces:

- `compiler/` - Rust compiler implementation and tests.
- `semantic-corpus/` - portable executable/semantic examples, manifests, and
  receipts.
- `docs/`, `README.md`, `STATUS.md`, `DESIGN.md`, `SPECIFICATION.md`, and
  `TEST_RESULTS.md` - public claims and verification posture.
- `editors/` - editor integration sources and packaging.

Keep local-only unless deliberately scrubbed:

- `.env`, `.env.*`, `.warden-safe-cache/`, local build caches, generated logs,
  local profiling output, and machine-specific artifacts.
- Unreviewed generated compiler outputs, private experiments, customer/project
  code, or unpublished source corpora.

## Editing Rules

- Keep backend maturity precise. Do not describe an experimental backend as
  production-ready because a subset test passes.
- Preserve semantic-corpus coupling: every executable corpus program should have
  a manifest entry, expected stdout, a Rust execution test when applicable, and
  a receipt entry.
- Update docs and receipts when test counts, corpus coverage, or backend
  guarantees change.
- Prefer focused compiler commits: one backend behavior, one corpus program, one
  receipt guard, or one docs boundary per commit.
- Avoid broad renames across language docs, examples, compiler internals, and
  release packaging without a written checklist.

## Verification

For docs-only or boundary changes:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
git diff --check
```

For Rust backend semantic-corpus work:

```powershell
cargo test --manifest-path compiler/Cargo.toml generated_rust_runs --quiet
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_manifest --quiet
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt --quiet
```

For release-readiness claims or test-count changes:

```powershell
cargo test --manifest-path compiler/Cargo.toml --quiet
```

Before committing or pushing, scan changed files for credential-shaped content
and confirm `.env` remains ignored.
