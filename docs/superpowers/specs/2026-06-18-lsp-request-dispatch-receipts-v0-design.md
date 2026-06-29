# LSP Request-Dispatch Receipts v0 Design

Date: 2026-06-18
Status: Approved direction; written spec pending user review

## Purpose

BuildLang now has checked receipts for execution, substrate posture, MIR
representation, memory layout, module graph, and symbol graph evidence. Those
receipts prove compiler-side semantic artifacts, but they do not prove the
interactive surface where humans and tools ask the compiler questions and get
structured answers back.

The LSP module is the natural next bridge for a shared human/machine language:
editors, agents, and automation can all speak JSON-RPC over the same server
surface. Provider implementations already exist for completion, hover,
definition, references, document symbols, formatting, folding, code actions,
and diagnostics. The weaker part is evidence: there is no checked receipt that
exercises `buildc lsp` request dispatch end to end, and the LSP status docs
are stale relative to the current `server.rs` dispatch loop.

LSP Request-Dispatch Receipts v0 adds a checked artifact for deterministic raw
LSP request fixtures. `buildc corpus verify` should verify that selected LSP
requests are dispatched, return valid JSON-RPC envelopes, expose expected
capabilities, and keep known gaps explicit. This is an evidence layer, not a
claim of full VS Code production readiness.

## Current Evidence

- `compiler/src/lsp/server.rs` has a `run_server()` stdio loop and
  `handle_raw_message()` dispatch for lifecycle methods, text document sync,
  completion, hover, definition, references, document symbols, formatting, and
  folding.
- `LanguageServer` has provider-backed methods for diagnostics, completion,
  hover, definition, references, document symbols, code actions, formatting,
  rename, and folding.
- `build_initialize_result()` advertises text document sync, completion, hover,
  definition, references, document symbols, formatting, rename, and folding
  capabilities.
- The current raw dispatch loop uses simplified string extraction helpers such
  as `extract_id`, `extract_json_string`, `extract_uri`, and
  `extract_position` rather than a full JSON-RPC deserializer.
- `compiler/src/lsp/STATUS.md` still says the runner only dispatches lifecycle
  methods. That is no longer accurate and should be corrected in this slice.
- There are provider unit tests, but no CLI or corpus receipt proving reachable
  request dispatch through the raw LSP message path.

## Alternatives Considered

### Approach A: Update LSP Status Docs Only

Correct the stale status file and tutorial wording.

Tradeoff: cheap and useful, but it does not add machine-checkable evidence.
The active goal calls for surfaces where humans and machines speak the same
language natively; prose alone is not enough.

### Approach B: Replace the Server With Full JSON-RPC Deserialization First

Introduce a proper typed JSON-RPC parser and convert request handling before
adding receipts.

Tradeoff: this is architecturally attractive, but it is a larger behavior
change. It risks conflating two questions: whether dispatch is reachable today
and whether the parser is production-grade. v0 should first pin the current
behavior with evidence, then a later v1 can harden parsing behind tests.

### Approach C: Add Checked LSP Request-Dispatch Receipts

Create `buildlang-lsp-dispatch-receipt/v0`, verify selected raw request
fixtures during `buildc corpus verify`, reference the receipt from substrate
evidence, and update LSP docs to match the verified scope.

Tradeoff: one more receipt family and verifier path. Benefit: the editor/agent
surface becomes auditable without overstating VS Code readiness.

Recommendation: Approach C.

## Architecture

The first implementation should add:

- `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`
- a focused receipt builder/verifier, ideally `compiler/src/lsp_dispatch.rs`
  or `compiler/src/lsp/receipt.rs` if that better matches local module layout
- deterministic raw LSP fixtures driven through the same dispatch function used
  by the server loop
- a substrate `interaction_surface` or `lsp_surface` block referencing the
  receipt
- `buildc corpus verify` integration
- updated LSP status/tutorial wording

The receipt builder should instantiate a fresh `LanguageServer`, replay a
small ordered fixture sequence, normalize JSON responses into deterministic
fields, and compare those fields against the checked receipt.

The implementation should make the raw dispatch seam reusable by exposing a
small public `dispatch_raw_message()` wrapper from the `buildlang` library.
That lets the `buildc` binary receipt verifier replay the same raw request
path without opening a real stdio transport in tests.

## Fixture Sequence

v0 should cover a minimal but representative editor session:

1. `initialize`
2. `initialized`
3. `textDocument/didOpen` with a small `.bld` source containing at least one
   function and one call site
4. `textDocument/documentSymbol`
5. `textDocument/completion`
6. `textDocument/hover`
7. `textDocument/definition`
8. `textDocument/references`
9. `textDocument/formatting`
10. `textDocument/foldingRange`
11. `textDocument/didChange`
12. `shutdown`
13. `exit`

If a capability currently returns an empty-but-valid result, the receipt may
record that honestly. It must not forge richer behavior than the server
actually returns.

Code actions and rename can remain known gaps in v0 if the raw dispatch loop
does not currently expose them. If they are already reachable with stable
payloads, they may be included, but the receipt should stay small enough to be
reviewable.

## Receipt Shape

The schema should be:

```json
{
  "schema": "buildlang-lsp-dispatch-receipt/v0",
  "receipt_id": "lsp-dispatch-semantic-corpus-2026-06-18",
  "created_at": "2026-06-18",
  "compiler": "buildc",
  "language": "buildlang",
  "source_set": {
    "kind": "semantic-corpus",
    "manifest": "manifest.json",
    "program_count": 8
  },
  "lsp_model": {
    "protocol": "LSP JSON-RPC over stdio",
    "dispatch": "buildc lsp raw message dispatch",
    "request_parser": "simplified string extraction",
    "semantic_anchor": "LSP providers over DocumentStore",
    "symbol_anchor": "receipts/symbol-graph-2026-06-18.json",
    "module_anchor": "receipts/module-graph-2026-06-18.json"
  },
  "fixtures": [
    {
      "id": "initialize",
      "method": "initialize",
      "response_kind": "response",
      "result_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "observed": {
        "has_result": true,
        "diagnostics": 0,
        "completion_items": 0,
        "document_symbols": 0,
        "locations": 0,
        "text_edits": 0,
        "folding_ranges": 0
      }
    }
  ],
  "summary": {
    "fixture_count": 13,
    "methods": [
      "initialize",
      "textDocument/didOpen",
      "textDocument/completion"
    ],
    "response_kinds": ["notification", "response", "none"],
    "known_gaps": [
      "full JSON-RPC deserialization",
      "full VS Code extension readiness",
      "compiler type-checker diagnostics in LSP"
    ]
  }
}
```

The checked receipt may store full normalized response JSON when the payload is
small. If payloads are verbose, it should store stable digest plus selected
counts and capability booleans. Either way, `corpus verify` must recompute the
same projection.

## Validation Rules

The verifier should reject the receipt when:

- `schema` is not `buildlang-lsp-dispatch-receipt/v0`.
- `compiler` is not `buildc`.
- `language` is not `buildlang`.
- `source_set.kind` is not `semantic-corpus`.
- `source_set.manifest` does not point at `manifest.json`.
- `source_set.program_count` differs from the manifest.
- `lsp_model` claims full JSON-RPC deserialization, full VS Code readiness, or
  compiler type-checker diagnostics before those are implemented and tested.
- Any fixture method, order, response kind, digest, observed count, known gap,
  or summary field differs from recomputed evidence.
- The substrate LSP/interaction surface references a missing or escaping
  receipt path.

Diagnostics should name the surface:

- `lsp dispatch receipt has unsupported schema 'x'`
- `lsp dispatch source_set.program_count mismatch`
- `lsp dispatch fixture completion result_digest mismatch`
- `lsp dispatch fixture document_symbol observed drift`
- `lsp dispatch summary drift`
- `substrate lsp_surface.lsp_receipt path not found: ...`

## Error Handling

The verifier should follow the existing receipt style: quiet helper functions
return `Result<T, String>`, and CLI wrappers print one actionable diagnostic to
stderr and return nonzero.

Malformed fixture JSON should fail the receipt build with the fixture id and
method named. Unknown request methods should be representable as intentional
fixtures only if the expected result is a JSON-RPC method-not-found error.

## Documentation Updates

Update `compiler/src/lsp/STATUS.md` to distinguish:

- implemented provider methods;
- raw dispatch methods currently reachable;
- current parsing limitation: simplified string extraction, not full JSON-RPC
  deserialization;
- current semantic limitation: diagnostics are provider/text-pattern based,
  not compiler type-checker diagnostics;
- receipt status once v0 is implemented.

Update `docs/tutorial.md` so it no longer says request dispatch is limited to
lifecycle methods, while still warning that the LSP is not a full VS Code
production experience.

## Testing

Implementation should be test-first:

- Unit test: raw initialize request returns advertised capabilities.
- Unit test: raw didOpen returns diagnostics notification for an opened
  document.
- Unit test: raw documentSymbol returns at least the known function symbol for
  the fixture document.
- Unit test: raw completion, hover, definition, references, formatting, and
  folding requests return valid JSON-RPC response envelopes.
- CLI test: valid LSP dispatch receipt passes `buildc corpus verify` and
  prints `lsp dispatch receipt: ok`.
- CLI test: wrong schema fails.
- CLI test: fixture digest drift fails.
- CLI test: fixture observed-count drift fails.
- CLI test: summary drift fails.
- CLI test: substrate rejects missing or escaping LSP receipt path.
- Existing corpus, substrate, module graph, symbol graph, MIR, and memory
  receipt slices continue to pass.

Targeted verification commands:

```text
cargo test --manifest-path compiler/Cargo.toml --bin buildc lsp --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus
cargo fmt --manifest-path compiler/Cargo.toml -- --check
git diff --check
```

## Non-Goals

- No full VS Code extension readiness claim.
- No full JSON-RPC parser replacement in v0.
- No compiler type-checker backed LSP diagnostics in v0.
- No semantic tokens, code lens, workspace symbol, or execute command receipt.
- No latency or responsiveness receipt yet.
- No package public API receipt.
- No networked registry or package download behavior.
- No cryptographic signing layer.

## Acceptance Criteria

This design is implemented when:

- `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json` exists and uses
  `buildlang-lsp-dispatch-receipt/v0`.
- The substrate receipt references it through an LSP/interaction surface.
- `buildc corpus verify` recomputes and validates the LSP dispatch receipt.
- Successful corpus verification prints `lsp dispatch receipt: ok`.
- Invalid fixtures cover schema drift, fixture digest drift, observed-count
  drift, summary drift, and substrate reference drift.
- `compiler/src/lsp/STATUS.md` and `docs/tutorial.md` accurately describe the
  verified dispatch scope and remaining gaps.
- Narrow implementation test slices and formatting checks pass.

## Future Extensions

Later slices can build on this receipt family:

- typed JSON-RPC deserialization receipts;
- compiler type-checker diagnostic receipts for LSP;
- semantic token receipts;
- code action and rename receipts;
- VS Code extension launch receipts;
- LSP latency/responsiveness receipts;
- agent-facing query protocol receipts over the same symbol/module evidence.

The invariant is that interactive language-server claims should be checked the
same way compiler artifacts are checked: deterministic request fixtures,
machine-readable receipts, and explicit known gaps.
