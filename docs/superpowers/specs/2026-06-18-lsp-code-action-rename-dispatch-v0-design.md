# LSP Code Action + Rename Dispatch v0 Design

## Goal

Wire the existing LSP code action and rename providers into raw JSON-RPC
dispatch, then receipt-verify them in the semantic corpus. This reduces the
remaining `Full LSP request dispatch for every provider method` gap without
claiming full VS Code readiness.

## Current Evidence

- `compiler/src/lsp/STATUS.md` says code actions and rename have provider
  methods but are not wired into the raw dispatch loop.
- `compiler/src/lsp/server.rs` already implements `LanguageServer::code_action`
  and `LanguageServer::rename`.
- `compiler/src/lsp/message.rs` already defines `CodeActionParams`,
  `CodeActionContext`, and `RenameParams`.
- `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json` verifies many core
  raw dispatch methods but does not include `textDocument/codeAction` or
  `textDocument/rename`.

## Scope

This slice only wires existing provider behavior through raw JSON-RPC dispatch.
It does not redesign quick-fix quality, add prepare-rename, implement typed LSP
deserialization for all methods, or claim end-to-end VS Code verification.

## Architecture

Extend `compiler/src/lsp/jsonrpc.rs` with structural accessors for:

- `params.range`
- `params.context.diagnostics[]`
- `params.context.only`
- `params.context.triggerKind`
- `params.newName`

Add a focused `compiler/src/lsp/response_json.rs` module to serialize the new
response types:

- `Vec<CodeAction>` as a JSON array with `title`, `kind`, `isPreferred`, and
  optional `edit`
- `WorkspaceEdit` as `{ "changes": { "<uri>": [TextEdit...] } }`
- `TextEdit`, `Range`, and `Position`

Keep the existing response builders in `server.rs` for older methods. The new
module keeps this slice from adding more serialization logic to the already
large server file.

## Dispatch Behavior

`textDocument/codeAction`:

- Builds `CodeActionParams` from parsed params.
- Missing `range` defaults to point range at `0:0`.
- Missing diagnostics defaults to an empty list.
- Returns an empty array when the document is not open or no diagnostics match.

`textDocument/rename`:

- Builds `RenameParams` from parsed params.
- Missing `newName` defaults to an empty string.
- Returns `null` when the document is not open or the cursor has no symbol.
- Returns a workspace edit when a symbol is found.

## Receipt Contract

The LSP dispatch receipt will add fixtures for:

- `textDocument/codeAction`
- `textDocument/rename`

Observed counts will gain:

- `code_actions`
- `workspace_edits`

The receipt remains honest by keeping `full VS Code extension readiness` and
`compiler type-checker diagnostics in LSP` as known gaps.

## Tests

Tests must prove:

- code action raw dispatch returns the existing quick fix for a supplied
  diagnostic
- rename raw dispatch returns workspace edits for all occurrences of the symbol
  at the requested position
- the LSP dispatch receipt rejects fixture, summary, and observed-count drift
  for the two new methods

## Verification

Targeted verification commands:

- `cargo fmt --manifest-path compiler/Cargo.toml -- --check`
- `cargo test --manifest-path compiler/Cargo.toml --lib code_action --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --lib raw_dispatch --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --bin quantac lsp_dispatch --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture`
- `cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus`
- `git diff --check`
