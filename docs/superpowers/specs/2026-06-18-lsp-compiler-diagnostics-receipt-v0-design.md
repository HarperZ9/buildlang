# LSP Compiler Diagnostics Receipt v0 Design

## Goal

Make the existing LSP diagnostics pipeline prove compiler-backed diagnostics in
the semantic-corpus LSP dispatch receipt. The receipt should demonstrate that
`textDocument/publishDiagnostics` can carry a diagnostic produced by the
lexer/parser/type-checker path, then remove the stale
`compiler type-checker diagnostics in LSP` known gap.

## Current Evidence

- `compiler/src/lsp/diagnostics.rs` already runs `Lexer`, `Parser`, and
  `TypeChecker` inside `DiagnosticsProvider::compute`.
- `compiler/src/lsp/server.rs` publishes diagnostics after `didOpen`,
  `didChange`, and `didSave`.
- `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json` currently records
  only a generic diagnostic count for `didOpen`.
- `compiler/src/lsp_dispatch.rs` still lists
  `compiler type-checker diagnostics in LSP` as a known gap.
- CLI drift tests already reject stale LSP model metadata and stale summary
  known gaps.

## Scope

This slice adds evidence for compiler-backed diagnostics through the raw LSP
dispatch receipt. It does not redesign diagnostic quality, replace heuristic
syntax checks, add typed LSP request structs, implement pull diagnostics, or
claim end-to-end VS Code readiness.

## Architecture

Keep the existing `DiagnosticsProvider::compute` pipeline as the source of
truth. Give compiler-pipeline diagnostics stable LSP `source` values so receipt
observation does not depend on matching localized message text:

- `quantalang/lexer`
- `quantalang/parser`
- `quantalang/type-checker`

Add receipt observation fields in `compiler/src/lsp_dispatch/model.rs` and
extraction logic in `compiler/src/lsp_dispatch/fixture.rs` so a diagnostics
notification can distinguish:

- total diagnostics;
- compiler diagnostics from the lexer/parser/type-checker pipeline;
- type-checker diagnostics from `quantalang/type-checker`.

The first implementation should avoid storing full diagnostic payloads. Stable
counts are enough for v0 and keep the receipt reviewable.

## Fixture Behavior

Add a deterministic raw LSP fixture after the existing healthy-session fixture
sequence, before shutdown:

1. Open or change a document to a source that reaches `TypeChecker` and
   produces a type-checker diagnostic.
2. Observe the returned `textDocument/publishDiagnostics` notification.
3. Record at least one compiler-backed diagnostic separately from heuristic
   formatting hints, bracket checks, or manually supplied code-action
   diagnostics.

The fixture source should be short, syntactically valid, and should not depend
on workspace files. The fixture id should make the source clear, for example
`did-change-type-error`.

## Receipt Contract

The LSP dispatch receipt will retain schema
`quantalang-lsp-dispatch-receipt/v0` and add these observed fields:

- `compiler_diagnostics`
- `type_errors`

The summary `known_gaps` must remove
`compiler type-checker diagnostics in LSP` only after the checked receipt proves
compiler-backed diagnostics. It must keep `full VS Code extension readiness`.

The receipt must continue to reference:

- `request_parser`: `serde_json structural JSON-RPC parser`
- `symbol_anchor`: `receipts/symbol-graph-2026-06-18.json`
- `module_anchor`: `receipts/module-graph-2026-06-18.json`

## Validation Rules

The existing rebuild-and-compare validator remains the enforcement mechanism.
Add drift tests that fail when:

- the compiler-diagnostic observed count is changed to zero;
- the removed known gap is reintroduced;
- the diagnostic fixture is missing, reordered unexpectedly, or has a digest
  mismatch through the existing fixture comparison path.

Validation error messages should follow existing style, for example
`lsp dispatch fixture did-change-type-error observed drift` and
`lsp dispatch summary drift`.

## Documentation Updates

Update `compiler/src/lsp/STATUS.md` to say compiler-backed diagnostics are
receipt-verified through the LSP dispatch fixture. Keep the remaining limits:

- full typed LSP deserialization is not complete;
- end-to-end VS Code behavior is not receipt-verified;
- diagnostic quality still includes heuristic checks and is not a complete IDE
  diagnostics engine.

Update `docs/tutorial.md` so the LSP feature list mentions compiler-backed
diagnostics in the verified raw dispatch surface, without implying full VS Code
extension readiness.

## Tests

Tests must be written first and prove:

- `DiagnosticsProvider::compute` emits at least one compiler-backed diagnostic
  for the chosen fixture source;
- raw dispatch returns a diagnostics notification with that compiler diagnostic
  after `didOpen` or `didChange`;
- the LSP dispatch receipt observes that compiler diagnostic;
- corpus verification rejects compiler-diagnostic observed-count drift;
- corpus verification rejects reintroducing the stale compiler-diagnostics
  known gap.

## Verification

Targeted verification commands:

- `cargo fmt --manifest-path compiler/Cargo.toml -- --check`
- `cargo test --manifest-path compiler/Cargo.toml --lib diagnostics --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --lib raw_dispatch --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --bin quantac lsp_dispatch --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture`
- `cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus`
- `git diff --check`

## Non-Goals

- No VS Code launch or extension packaging receipt.
- No pull-diagnostics protocol implementation.
- No complete typed LSP request deserialization.
- No diagnostic latency or responsiveness claim.
- No broad diagnostics refactor unless a small extraction is required to make
  compiler diagnostics observable.

## Acceptance Criteria

The slice is complete when:

- the LSP dispatch receipt includes a deterministic compiler-backed diagnostics
  fixture;
- the receipt records nonzero compiler diagnostic evidence;
- `compiler type-checker diagnostics in LSP` is no longer a known gap;
- stale observed counts and stale known-gap summaries fail corpus verification;
- LSP status docs and tutorial wording match the verified scope;
- targeted verification commands pass.
