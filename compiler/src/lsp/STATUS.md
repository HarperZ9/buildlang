# Status: lsp/

Last audited: 2026-06-18

## Working
- **Transport** (`transport.rs`, 475 lines): Stdio-based LSP transport with Content-Length header parsing, raw message send/receive. 5 unit tests.
- **Document Store** (`document.rs`, 529 lines): Tracks open documents, versions, content changes. Supports `didOpen`, `didChange`, `didClose`. 4 unit tests.
- **Types** (`types.rs`, 1111 lines): Full LSP type definitions (Position, Range, Location, Diagnostic, CompletionItem, etc.).
- **Message Types** (`message.rs`, 770 lines): Request/response/notification message structures for LSP protocol.
- **Diagnostics** (`diagnostics.rs`, 660 lines): Syntax checking, bracket matching, common issue detection, unused variable detection, and compiler pipeline diagnostics from lexer, parser, and type-checker errors. The filtered diagnostics slice currently lists 5 tests.
- **Completion** (`completion.rs`, 564 lines): Keyword and builtin type completion suggestions. Has 2 `todo!()` calls for context-aware completion. 4 unit tests.
- **Hover** (`hover.rs`, 269 lines): Keyword documentation, builtin type docs, local definition lookup. Has 2 `todo!()` calls.
- **Symbols** (`symbols.rs`, 664 lines): Document symbol extraction (functions, structs, enums, etc.).
- **Definition** (`definition.rs`, 435 lines): Go-to-definition via symbol search across documents.
- **Code Actions** (`actions.rs`, 435 lines): Quick fixes and refactoring suggestions.
- **Server** (`server.rs`): Main server with lifecycle management and raw request dispatch for lifecycle, document sync, completion, hover, definition, references, document symbols, formatting, folding ranges, code actions, rename, and unknown-method errors.
- **Dispatch receipt** (`semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`): Verifies a fixed raw LSP fixture sequence through `corpus verify`, including initialize, initialized, didOpen, documentSymbol, completion, hover, definition, references, formatting, foldingRange, didChange, compiler-backed type diagnostics, codeAction, rename, shutdown, and exit.

## Partial
- **Server runner** (`run_server()` in `server.rs`): The stdio transport loop dispatches the same raw message path covered by the LSP dispatch receipt, using a `serde_json` structural JSON-RPC parser for method, id, and common params. Params still flow through lightweight `serde_json::Value` accessors rather than typed request structs for every method.

## Aspirational
- Full VS Code extension integration: `quantac lsp` starts the current server loop and dispatches several core requests, but the end-to-end VS Code language-server experience is not yet receipt-verified.
- Full typed LSP deserialization: the server now parses JSON structurally, but still maps params through lightweight `serde_json::Value` accessors instead of typed request structs for every method.

## Not Started
- Published VS Code extension package/release artifact.

## Honest Assessment
The LSP module has real implementations for major language-server capabilities, and the raw `quantac lsp` dispatch path now has a semantic-corpus receipt for a representative request sequence through structural JSON-RPC parsing and compiler-backed diagnostics. The important remaining limits are typed request coverage and end-to-end VS Code verification.
