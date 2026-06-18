# LSP Workspace Symbols v0 Design

Date: 2026-06-18

## Context

The LSP server now exposes a checked raw JSON-RPC surface for core editor
requests, compiler-backed diagnostics, rename, code actions, and semantic
tokens v0. The symbol layer has one remaining low-risk gap: `SymbolProvider`
already implements `workspace_symbols(query)`, and `ServerCapabilities::full()`
sets `workspace_symbol_provider` in the Rust type model, but raw
`workspace/symbol` dispatch is not routed or receipt-verified.

This matters for the larger QuantaLang goal because workspace symbol search is
the editor-facing bridge between source names, machine-readable symbol
identity, and navigation. It makes the language more discoverable to humans and
tools without claiming a full compiler-backed global index.

## Goal

Wire the existing workspace symbol provider into `quantac lsp` raw dispatch and
prove it in the semantic-corpus LSP dispatch receipt.

The implementation should:

- Advertise `workspaceSymbolProvider` in initialize responses.
- Decode `workspace/symbol` params through the raw-boundary helper layer.
- Route successful requests to the existing `SymbolProvider::workspace_symbols`
  behavior.
- Serialize `SymbolInformation[]` responses using stable LSP fields.
- Extend the checked LSP dispatch receipt with nonzero workspace-symbol
  observation.

## Non-Goals

- Do not build a new compiler-backed workspace index in this slice.
- Do not scan unopened files from disk.
- Do not add workspace folders, project graph loading, or package registry
  integration.
- Do not change document symbol extraction behavior.
- Do not claim full VS Code extension readiness.
- Do not claim resolved type/effect/module identities for workspace symbols.

## Architecture

Keep the change inside the existing raw LSP flow:

1. Client opens a document with `textDocument/didOpen`.
2. Client sends `workspace/symbol` with `params.query`.
3. `raw_params::decode_workspace_symbol_query` validates and returns the query.
4. `LanguageServer::workspace_symbol(&query)` delegates to
   `self.symbols.workspace_symbols(query)`.
5. `server.rs` serializes the result as a JSON-RPC response containing a flat
   array of `SymbolInformation` objects.

No new provider is needed. The existing `SymbolProvider` already owns the
source-to-symbol extraction and workspace document iteration.

## Param Decoding

Add this helper to `compiler/src/lsp/raw_params.rs`:

```rust
pub fn decode_workspace_symbol_query(
    message: &JsonRpcMessage,
) -> Result<String, RawParamError>
```

Rules:

- `params.query` is required.
- `params.query` must be a string.
- Empty query is accepted and returns all currently opened document symbols,
  matching the existing provider's substring behavior.
- Missing or non-string query returns JSON-RPC `-32602 Invalid params` with
  detail `params.query is required`.

## Response JSON

Add a serializer for `SymbolInformation` values. Each item should include:

- `name`
- `kind`
- `location` with `uri` and `range`
- `tags` only when non-empty
- `containerName` only when present

The serializer may live in `server.rs` beside existing document-symbol JSON, or
in `response_json.rs` if implementation would otherwise grow `server.rs`
unnecessarily. It must use the existing `Range`/`Location` shape and stable
ordering from the provider result.

## Raw Dispatch

Add a `workspace/symbol` branch in `handle_raw_message`:

- Require a request ID.
- Decode `params.query`.
- Return `-32602` on malformed params.
- Call `server.workspace_symbol(&query)`.
- Return a JSON array; an unmatched query returns `[]`.

Initialize JSON should include:

```json
"workspaceSymbolProvider": true
```

The Rust `ServerCapabilities` type already carries this capability; this slice
must make the actual initialize response match it.

## Receipt Integration

Extend `compiler/src/lsp_dispatch/fixture.rs` with a `workspace/symbol` request
after `didOpen` and before document mutation. Use a query that is stable against
the current fixture source, such as `"help"` for `helper`.

Extend `LspDispatchObserved` with:

```rust
workspace_symbols: usize
```

The observer should count `result.len()` for `workspace/symbol` responses. The
checked receipt should record a nonzero `workspace_symbols` count and include
`workspace/symbol` in the sorted method summary.

## Testing

Raw dispatch tests:

- Initialize response advertises `workspaceSymbolProvider`.
- Opened fixture document plus `workspace/symbol` query returns a symbol named
  `helper`.
- Unmatched query returns an empty array.
- Missing `params.query` returns `-32602 Invalid params`.

Receipt tests:

- Built LSP dispatch receipt contains a `workspace-symbol` fixture.
- The fixture observes `workspace_symbols > 0`.
- CLI corpus verification rejects workspace-symbol observed drift.

Verification commands:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin quantac lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

## Documentation Updates

After implementation, update:

- `compiler/src/lsp/STATUS.md`
- `STATUS.md`
- `docs/tutorial.md`

The docs should say workspace symbol dispatch is receipt-verified for opened
documents. They should keep full compiler-backed global indexing and
end-to-end VS Code behavior as open gaps.

## Success Criteria

- `workspace/symbol` is reachable through raw `quantac lsp` dispatch.
- Valid requests return flat LSP `SymbolInformation[]` for currently opened
  documents.
- Malformed requests return `-32602`.
- The semantic-corpus LSP dispatch receipt verifies a nonzero workspace-symbol
  observation.
- Targeted LSP and corpus verification commands pass.
- Documentation reflects the v0 scope without claiming global compiler-backed
  indexing.
