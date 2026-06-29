# Typed Raw LSP Param Decoding Design

Date: 2026-06-18

## Context

`buildc lsp` now dispatches a representative raw JSON-RPC request sequence and
the semantic corpus verifies that path through
`semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`. The remaining typed
coverage gap is narrower: `compiler/src/lsp/server.rs` still converts request
params to existing LSP structs inline with `JsonRpcMessage` accessors, using
defaults such as empty URI, version `0`, position `0:0`, and empty rename names
when required params are missing.

That keeps the dispatch path working, but it hides malformed-client evidence and
keeps request decoding logic mixed into the router. The next slice should make
the raw boundary more explicit without changing provider semantics or claiming
VS Code readiness.

## Goal

Add a typed decoding layer for the raw JSON-RPC methods that already dispatch in
`handle_raw_message`, then route successful decodes into the existing typed
server methods.

The design moves the codebase toward the "Full typed LSP deserialization" line
in `compiler/src/lsp/STATUS.md` while preserving the existing LSP dispatch
receipt behavior for valid requests.

## Non-Goals

- Do not rewrite the server router into a method enum.
- Do not derive `serde::Deserialize` across the full LSP type tree.
- Do not add new LSP features.
- Do not modify completion, hover, symbol, diagnostic, code action, formatting,
  rename, or folding-range provider behavior.
- Do not claim end-to-end VS Code extension verification.
- Do not change the semantic corpus fixture sequence unless valid-request output
  changes for a justified reason.

## Proposed Architecture

Add a new focused module:

`compiler/src/lsp/raw_params.rs`

The module owns conversion from `JsonRpcMessage` into the existing typed request
structs:

- `InitializeParams`
- `DidOpenTextDocumentParams`
- `DidChangeTextDocumentParams`
- `DidSaveTextDocumentParams`
- `DidCloseTextDocumentParams`
- `CompletionParams`
- `TextDocumentPositionParams`
- `CodeActionParams`
- `DocumentFormattingParams`
- `RenameParams`
- document URI parameters for `documentSymbol` and `foldingRange`

The module should expose small functions named by method intent, such as
`decode_did_open`, `decode_text_document_position`, `decode_code_action`, and
`decode_rename`. Each function returns `Result<T, RawParamError>`.

`RawParamError` should carry a concise field path and message, for example:

```text
params.textDocument.uri is required
params.position.line must be a non-negative integer
params.newName is required
```

`server.rs` keeps response construction and dispatch ownership. For request
methods with an ID, decode failure returns JSON-RPC `-32602 Invalid params`.
For notifications, decode failure should also produce a JSON-RPC error response
when an ID is present in the malformed payload; otherwise it should return
`None`, matching notification no-response semantics.

## Decoder Rules

Required fields:

- Text document URI is required for all document-targeting methods.
- `didOpen` requires `textDocument.uri`, `languageId`, `version`, and `text`.
- `didChange` requires `textDocument.uri`, `textDocument.version`, and at least
  one `contentChanges[].text` value.
- Position-based requests require `position.line` and `position.character`.
- `rename` requires `newName` and must not silently replace with an empty string.
- `codeAction` requires `range` and `context`; diagnostics default to an empty
  list only when the context exists and omits diagnostics.

Accepted defaults:

- `InitializeParams.capabilities` may continue to use
  `ClientCapabilities::default()` until the project adopts a fuller capability
  model.
- `formatting.options` may continue to use `FormattingOptions::default()` if the
  client omits options.
- `completion.context` may remain `None` unless the project later needs trigger
  metadata.
- `didSave.text` may remain `None`.

Numeric fields must reject negative values before casting to `u32`.

## Error Handling

Add a helper in `server.rs` or `raw_params.rs` that maps `RawParamError` to:

```json
{"jsonrpc":"2.0","id":<id>,"error":{"code":-32602,"message":"Invalid params: <detail>"}}
```

Parse errors remain `-32700`. Unknown methods remain `-32601`.

This makes malformed raw requests observable without treating them as valid
requests against empty documents.

## Testing

Use TDD. Add failing tests before production changes.

Initial RED tests should cover:

- `didOpen` missing `textDocument.uri` returns `-32602`.
- `hover` with negative `position.line` returns `-32602`.
- `rename` missing `newName` returns `-32602`.
- `codeAction` missing `context` returns `-32602`.
- Valid existing dispatch tests still pass after decoder integration.

Implementation verification should include:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git diff --check
```

## Documentation

After implementation, update `compiler/src/lsp/STATUS.md`:

- Move the server-runner note from "params still flow through lightweight
  accessors" to "params decode through typed raw-boundary helpers for the
  currently dispatched methods."
- Keep full VS Code extension integration as aspirational.
- Keep full serde-backed LSP request coverage as an open future direction if not
  all type-tree structs derive deserialize.

## Acceptance Criteria

- Raw dispatch no longer silently accepts missing required params for the
  methods listed in this spec.
- Valid dispatch receipt behavior remains stable.
- Invalid-param behavior is covered by tests.
- The new decoder code is focused and can be tested without invoking providers.
- `corpus verify` still reports `lsp dispatch receipt: ok`.
