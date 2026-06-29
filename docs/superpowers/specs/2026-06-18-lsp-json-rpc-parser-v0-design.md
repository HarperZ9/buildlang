# LSP JSON-RPC Parser v0 Design

## Goal

Replace the current exact-substring LSP raw-message dispatch with a small
structural JSON parser so `buildc lsp` accepts normal JSON-RPC formatting:
pretty-printed payloads, reordered fields, string or numeric IDs, and nested
`params` objects.

## Current Evidence

- `compiler/src/lsp/server.rs` dispatches by `content.contains("\"method\":\"...\"")`.
- `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json` records the parser as
  `simplified string extraction`.
- `compiler/src/lsp/STATUS.md` lists real JSON parsing as a remaining gap.

## Scope

This slice upgrades request parsing and preserves existing handler semantics.
It does not add new LSP capabilities, full typed LSP deserialization, request
batching, cancellation, or end-to-end VS Code verification.

## Architecture

Add a focused `compiler/src/lsp/jsonrpc.rs` module that parses a raw payload
with `serde_json::Value` and exposes small accessor methods for the existing
dispatch path:

- request ID as raw JSON text for the existing response builder
- method string
- nested string and integer params
- `textDocument.uri`
- `position.line` and `position.character`
- first `contentChanges[].text`

`server.rs` will parse once at the start of dispatch, route on the parsed
method, and use typed accessors instead of scanning the whole content string.
Malformed JSON requests with an ID return JSON-RPC `-32700`; malformed
notifications with no recoverable ID return `None`.

## Receipt Contract

The LSP dispatch receipt should update:

- `lsp_model.request_parser`: `serde_json structural JSON-RPC parser`
- summary known gaps: remove `full JSON-RPC deserialization`

The receipt remains honest by keeping `full VS Code extension readiness` and
`compiler type-checker diagnostics in LSP` until those are separately verified.

## Tests

Tests must prove the parser no longer depends on compact field order:

- pretty-printed `initialize` returns capabilities
- string ID is preserved in responses
- reordered `didOpen` + pretty `documentSymbol` returns symbols
- malformed JSON with an ID returns `-32700`
- malformed JSON notification returns no response
- corpus verifier rejects stale parser metadata or stale receipt digest

## Verification

Targeted verification commands:

- `cargo fmt --manifest-path compiler/Cargo.toml -- --check`
- `cargo test --manifest-path compiler/Cargo.toml --lib json_rpc --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --bin buildc lsp_dispatch --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture`
- `cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus`
- `git diff --check`
