# LSP Semantic Tokens v0 Design

Date: 2026-06-18

## Context

`quantac lsp` now starts the stdio server loop and routes a checked raw
JSON-RPC fixture sequence through structural parsing, typed raw-boundary param
helpers, and existing providers. The semantic-corpus LSP receipt proves
lifecycle, document sync, completion, hover, definition, references, document
symbols, formatting, folding ranges, code actions, rename, and compiler-backed
diagnostics.

The editor-facing semantic surface is still incomplete. `ServerCapabilities`
already has a semantic-token provider slot, and the LSP method table names
`textDocument/semanticTokens/full`, but the capability is disabled and the raw
dispatch path returns method-not-found for semantic token requests.

Semantic tokens are the next useful LSP slice because they expose QuantaLang's
symbols and syntax to both humans and tools as a machine-readable stream. This
moves the language toward a shared human/machine surface without claiming that
full VS Code extension behavior, complete semantic indexing, or unverified
native/GPU backends are done.

## Goal

Add v0 full-document semantic token support for open QuantaLang documents.

The implementation should:

- Advertise `semanticTokensProvider` in initialize responses with a stable
  legend.
- Dispatch `textDocument/semanticTokens/full` through the existing raw
  JSON-RPC path.
- Return LSP-compatible encoded token data for the currently open document.
- Extend the checked LSP dispatch receipt so `quantac corpus verify` proves the
  semantic-token request path.
- Keep the feature honest: v0 is a responsive editor token surface, not a full
  compiler semantic database.

## Non-Goals

- Do not build a VS Code extension or claim end-to-end VS Code readiness.
- Do not add range semantic tokens, delta tokens, token refresh, or work-done
  progress.
- Do not replace the existing symbol graph, MIR representation, or memory
  layout receipts.
- Do not require a successful full parse before returning tokens.
- Do not infer every symbol's resolved type, trait, module path, or effect row.
- Do not change provider behavior for completion, hover, diagnostics,
  definition, references, document symbols, code actions, rename, formatting, or
  folding ranges.

## Recommended Architecture

Add a focused semantic-token provider:

`compiler/src/lsp/semantic_tokens.rs`

The module owns token extraction, legend ordering, LSP delta encoding, and unit
tests. `server.rs` stays responsible for routing, response construction, and
state access.

Core types:

```rust
pub struct SemanticTokensProvider;

pub struct SemanticTokens {
    pub data: Vec<u32>,
}

pub struct SemanticTokenLegendSpec {
    pub token_types: Vec<&'static str>,
    pub token_modifiers: Vec<&'static str>,
}
```

The provider consumes an opened `Document` and returns encoded full-document
tokens sorted by `(line, character)`.

## Token Legend

Use a stable v0 legend:

Token types:

- `namespace`
- `type`
- `function`
- `variable`
- `parameter`
- `property`
- `keyword`
- `comment`
- `string`
- `number`
- `operator`
- `macro`

Token modifiers:

- `declaration`
- `definition`
- `readonly`
- `static`
- `deprecated`

The legend is intentionally small and LSP-compatible. Future compiler-backed
semantic indexing can add more precision, but v0 should avoid legend churn once
receipt-verified.

## Classification Rules

The provider should be lexical plus declaration-aware:

- Comments beginning with `//` become `comment`.
- String literals become `string`.
- Numeric literals become `number`.
- Known QuantaLang keywords become `keyword`.
- These operator spans become `operator`: `+`, `-`, `*`, `/`, `%`, `=`, `==`,
  `!=`, `<`, `<=`, `>`, `>=`, `&&`, `||`, `!`, `->`, `=>`, `::`, `.`, `:`,
  `;`, `,`, `|`, and `&`.
- Identifiers after declaration introducers such as `fn`, `struct`, `enum`,
  `trait`, `impl`, `type`, `const`, and `let` receive declaration/definition
  modifiers where applicable.
- Function names in declarations and direct call positions receive `function`.
- Type-like identifiers in declarations, annotations, and constructor-like
  positions receive `type`.
- Macro invocations ending with `!` receive `macro`.
- Other identifiers default to `variable`.

Start with a small document-line scanner local to the provider. If a later
slice reuses lexer spans, it must preserve the same best-effort behavior:
malformed documents should still return tokens instead of failing the request.

## LSP Data Flow

1. Client opens a document with `textDocument/didOpen`.
2. Client sends `textDocument/semanticTokens/full` with
   `params.textDocument.uri`.
3. `raw_params::decode_document_uri` extracts the document URI.
4. `LanguageServer::semantic_tokens(&uri)` fetches the open document and calls
   `SemanticTokensProvider::full(&doc)`.
5. `server.rs` serializes the result as:

```json
{"data":[deltaLine,deltaStart,length,tokenType,tokenModifiers,...]}
```

If the document is not open, return `null` like other optional LSP providers.
Malformed params return JSON-RPC `-32602 Invalid params`.

## Encoding Rules

Tokens must follow LSP relative encoding:

- `deltaLine`: current token line minus previous token line.
- `deltaStart`: current token start character, or start minus previous start
  when on the same line.
- `length`: token length in UTF-16 code units.
- `tokenType`: index into the v0 legend token types.
- `tokenModifiers`: bitset over the v0 legend token modifiers.

Tokens must be sorted and must not overlap. If classification discovers an
overlap, the earlier token wins and the later overlapping token is skipped.

## Receipt Integration

Extend the existing LSP dispatch fixture sequence with one
`textDocument/semanticTokens/full` request after `didOpen` and before document
mutation. Extend the LSP dispatch observation model with:

```rust
semantic_tokens: usize
```

The observer should count `result.data.len() / 5` for semantic token responses.
The checked receipt should record a nonzero `semantic_tokens` count for the new
fixture. The receipt summary methods list should include
`textDocument/semanticTokens/full`.

The substrate receipt may continue pointing at the LSP dispatch receipt; no new
top-level substrate receipt schema is required for this v0 slice unless the
existing verifier requires updated method coverage.

## Error Handling

Semantic-token classification should be best-effort and should not emit
diagnostics. Diagnostics remain owned by `DiagnosticsProvider`.

Request-level errors:

- Missing or malformed `params.textDocument.uri`: `-32602 Invalid params`.
- Unknown document URI: successful response with `null`.
- Internal serialization failure: keep using infallible local JSON builders or
  `expect` only where existing response builders already assume serialization
  cannot fail.

## Testing

Unit tests:

- Provider encodes comments, keywords, function declarations, call identifiers,
  strings, and numbers into non-overlapping LSP token data.
- Provider computes correct relative deltas across multiple lines.
- Provider returns best-effort tokens for malformed source.

Raw dispatch tests:

- Initialize response advertises `semanticTokensProvider` with the v0 legend.
- `textDocument/semanticTokens/full` returns non-empty `data` for an opened
  fixture document.
- Missing `textDocument.uri` returns `-32602 Invalid params`.
- Unknown document URI returns `null`.

Receipt tests:

- Built LSP dispatch receipt includes a semantic-token fixture.
- Receipt observer records a nonzero `semantic_tokens` count.
- Receipt validation rejects semantic-token observed drift.

Verification commands:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib semantic_tokens --quiet
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin quantac lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

## Documentation Updates

Update these docs after implementation:

- `compiler/src/lsp/STATUS.md`: move semantic tokens from absent capability to
  partial or implemented v0 capability, and keep VS Code readiness as a known
  gap.
- `STATUS.md`: mention semantic-token receipt coverage in the LSP server line.
- `docs/tutorial.md`: list semantic tokens under LSP launch support only after
  receipt verification passes.

## Success Criteria

- `quantac lsp` initialize responses advertise a stable semantic-token legend.
- Raw `textDocument/semanticTokens/full` requests against opened documents
  return LSP-compatible encoded token data.
- The semantic-corpus LSP dispatch receipt includes and verifies semantic-token
  coverage.
- Targeted LSP and corpus verification commands pass.
- Documentation states the v0 capability and does not claim full VS Code
  extension readiness or full compiler-backed semantic indexing.
