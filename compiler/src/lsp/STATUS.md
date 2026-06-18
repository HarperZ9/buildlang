# Status: lsp/

Last audited: 2026-06-18

## Working
- **Transport** (`transport.rs`, 468 lines): Stdio-based LSP transport with Content-Length header parsing, raw message send/receive. 5 unit tests.
- **Document Store** (`document.rs`, 528 lines): Tracks open documents, versions, content changes. Supports `didOpen`, `didChange`, `didClose`. 4 unit tests.
- **Types** (`types.rs`, 1104 lines): Full LSP type definitions (Position, Range, Location, Diagnostic, CompletionItem, etc.).
- **Message Types** (`message.rs`, 771 lines): Request/response/notification message structures for LSP protocol.
- **Diagnostics** (`diagnostics.rs`, 487 lines): Syntax checking, bracket matching, common issue detection, unused variable detection. 4 unit tests.
- **Completion** (`completion.rs`, 568 lines): Keyword and builtin type completion suggestions. Has 2 `todo!()` calls for context-aware completion. 4 unit tests.
- **Hover** (`hover.rs`, 260 lines): Keyword documentation, builtin type docs, local definition lookup. Has 2 `todo!()` calls.
- **Symbols** (`symbols.rs`, 600 lines): Document symbol extraction (functions, structs, enums, etc.).
- **Definition** (`definition.rs`, 436 lines): Go-to-definition via symbol search across documents.
- **Code Actions** (`actions.rs`, 428 lines): Quick fixes and refactoring suggestions.
- **Server** (`server.rs`): Main server with lifecycle management and raw request dispatch for lifecycle, document sync, completion, hover, definition, references, document symbols, formatting, folding ranges, and unknown-method errors.
- **Dispatch receipt** (`semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`): Verifies a fixed raw LSP fixture sequence through `corpus verify`, including initialize, initialized, didOpen, documentSymbol, completion, hover, definition, references, formatting, foldingRange, didChange, shutdown, and exit.

## Partial
- **Server runner** (`run_server()` in `server.rs`): The stdio transport loop dispatches the same raw message path covered by the LSP dispatch receipt, but parsing is still manual string matching and string extraction, not full JSON-RPC deserialization. Code actions and rename have provider methods but are not wired into the raw dispatch loop.

## Aspirational
- Full VS Code extension integration: `quantac lsp` starts the current server loop and dispatches several core requests, but the end-to-end VS Code language-server experience is not yet receipt-verified.
- Real JSON parsing: the server uses manual string matching, not proper JSON deserialization.
- Semantic analysis integration: diagnostics are text-pattern-based (bracket matching, unused variables by regex), not driven by the actual lexer/parser/type-checker pipeline.

## Not Started
- Published VS Code extension package/release artifact.
- No integration with the compiler's type checker for semantic diagnostics.
- Full LSP request dispatch for every provider method.

## Honest Assessment
The LSP module has real implementations for major language-server capabilities, and the raw `quantac lsp` dispatch path now has a semantic-corpus receipt for a representative request sequence. The important remaining limit is protocol quality: the runner still uses manual string matching instead of robust JSON-RPC parsing, and VS Code behavior has not been verified end to end.
