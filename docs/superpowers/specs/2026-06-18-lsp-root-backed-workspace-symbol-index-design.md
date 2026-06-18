# LSP Root-Backed Workspace Symbol Index Design

Date: 2026-06-18

## Context

`workspace/symbol` is now reachable through raw `quantac lsp` dispatch and is
receipt-verified in `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`.
That implementation searches the `DocumentStore`, so it only sees documents
opened through LSP notifications.

The next useful step is project-backed discovery: a symbol search should find
symbols in ordinary `.quanta` files under the initialized workspace root even
when those files are not currently open in the editor. This moves the LSP
surface closer to a human/machine shared project map without claiming full
compiler-resolved type, effect, module, or package identity.

## Goal

Add a bounded root-backed workspace symbol index for raw `workspace/symbol`
requests.

The implementation should:

- Use `initialize.params.rootUri` as the workspace root when it maps to a local
  filesystem directory.
- Discover `.quanta` files under that root with deterministic ordering.
- Reuse the existing symbol parser so file-backed and opened-document-backed
  symbols have the same shape.
- Overlay opened documents over indexed files so unsaved editor state wins.
- Keep `workspace/symbol` responses as flat LSP `SymbolInformation[]`.
- Extend the checked LSP dispatch receipt with a workspace-symbol result from
  an unopened root file.

## Non-Goals

- Do not build a type-checker-backed or module-graph-backed symbol database in
  this slice.
- Do not resolve type, effect, trait, import, package, or module identities.
- Do not scan outside the initialized root.
- Do not scan the whole repository when `rootUri` is absent or unsupported.
- Do not index generated output, build artifacts, vendored package caches,
  `.git`, `target`, `node_modules`, `.worktrees`, or hidden infrastructure
  trees.
- Do not add live file watching or incremental filesystem invalidation yet.
- Do not claim end-to-end VS Code extension readiness.

## Recommended Approach

Add a small `WorkspaceSymbolIndex` owned by `LanguageServer`. The index stores
file-backed symbols derived from bounded root scanning and can be rebuilt on
initialize or on explicit test hooks.

This is intentionally not a compiler database. It is a deterministic project
symbol cache that delegates symbol extraction to `SymbolProvider`.

## Architecture

### Components

- `WorkspaceSymbolIndex`
  - owns the last indexed root path
  - owns a map from document URI to indexed `DocumentSymbol` values
  - records scan stats such as indexed file count and skipped file count

- `workspace_index.rs`
  - converts supported `file://` root URIs into local paths
  - recursively discovers `.quanta` files
  - applies exclusion rules and file caps
  - reads file contents and builds temporary `Document` values
  - asks `SymbolProvider` to parse each document

- `LanguageServer`
  - sets `root_uri` during initialize as today
  - rebuilds the index when initialize has a supported local root
  - answers `workspace_symbol(query)` by merging indexed symbols with opened
    document symbols

### Merge Rules

Opened documents must override indexed files by URI. This prevents stale
on-disk state from winning when the editor has unsaved changes.

Ordering should be deterministic:

1. opened document symbols, sorted by URI
2. indexed unopened-file symbols, sorted by URI
3. symbols in source order within each file

The query semantics remain the current case-insensitive substring match.
Empty query continues to return all available symbols within the bounded index.

## Root And URI Rules

Supported roots:

- `file:///...` URIs that decode to a local directory
- Windows drive paths represented as `file:///C:/...`

Unsupported roots:

- non-file schemes
- missing `rootUri`
- file URIs that do not resolve to a directory
- malformed percent escapes

Unsupported roots should not fail initialize. They should leave the index empty
and keep opened-document `workspace/symbol` behavior working.

URI output should be stable `file://` URIs derived from canonical local paths
where possible. If canonicalization fails for a readable file, the index should
skip that file rather than emitting an unstable URI.

## Scan Bounds

The first implementation should use conservative fixed caps:

- maximum indexed files: 512
- maximum bytes read per file: 256 KiB
- maximum recursion depth: 16 directory levels

If a cap is reached, the index should keep the symbols collected so far and
record a scan stat. It should not return an LSP error for ordinary cap hits.

Excluded directory names:

- `.git`
- `.worktrees`
- `target`
- `node_modules`
- `dist`
- `build`
- `.Codex`

Excluded files:

- non-`.quanta` files
- files larger than the byte cap
- files that cannot be read as UTF-8

## Error Handling

Indexing is best-effort. The LSP server should not fail initialize or
`workspace/symbol` because a file is unreadable, too large, non-UTF-8, or
inside an excluded tree.

Errors that should become test-visible scan stats:

- unsupported root URI
- read failure
- skipped large file
- skipped non-UTF-8 file
- file cap reached

The raw `workspace/symbol` param contract stays unchanged: missing or non-string
`params.query` returns `-32602 Invalid params`.

## Receipt Coverage

Extend the LSP dispatch fixture to create a temporary workspace root with two
files:

- `main.quanta`, opened through `didOpen`
- `library.quanta`, left unopened on disk

The `workspace/symbol` fixture should query a symbol that exists only in
`library.quanta`. The receipt should record a nonzero `workspace_symbols` count
for an unopened file-backed result.

The existing opened-document workspace-symbol test should remain. This proves
both paths:

- opened documents are still searchable
- unopened root files are now searchable

## Testing

Raw dispatch tests:

- initialize with local `rootUri` indexes an unopened `.quanta` file.
- `workspace/symbol` returns a symbol from an unopened indexed file.
- opened document content overrides an indexed file with the same URI.
- unsupported `rootUri` leaves the index empty but does not break opened
  document search.
- missing `params.query` still returns `-32602`.

Index unit tests:

- file URI decoding accepts Windows-style `file:///C:/...` roots.
- deterministic traversal sorts file paths.
- excluded directories are skipped.
- file count and file size caps are enforced.
- unreadable or non-UTF-8 files are skipped without panics.

Receipt tests:

- built LSP dispatch receipt contains a workspace-symbol fixture backed by an
  unopened file.
- CLI corpus verification rejects workspace-symbol observed drift.
- `corpus verify` accepts the refreshed receipt.

Verification commands:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib workspace_symbol --quiet
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

The docs should say `workspace/symbol` is root-backed for bounded local
workspace files and opened documents. They should still keep compiler-resolved
global symbol identity and end-to-end VS Code behavior as open gaps.

## Success Criteria

- `workspace/symbol` finds symbols in bounded unopened `.quanta` files below a
  supported local `rootUri`.
- Opened documents override indexed file contents for the same URI.
- Unsupported or absent roots preserve opened-document behavior.
- The semantic-corpus LSP dispatch receipt proves a nonzero workspace-symbol
  result from an unopened root file.
- Targeted LSP and corpus verification commands pass.
- Documentation accurately describes root-backed indexing without claiming
  compiler-resolved global identity.
