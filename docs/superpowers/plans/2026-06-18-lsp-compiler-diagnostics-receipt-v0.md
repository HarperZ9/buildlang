# LSP Compiler Diagnostics Receipt v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove compiler-backed LSP diagnostics in the semantic-corpus LSP dispatch receipt and remove the stale compiler-diagnostics known gap.

**Architecture:** Keep `DiagnosticsProvider::compute` as source of truth; tag lexer/parser/type-checker diagnostics with stable LSP source strings; move LSP dispatch observation into `observe.rs`; add `compiler_diagnostics` and `type_errors`; replay a deterministic type-error fixture; regenerate the checked receipt; update docs.

**Tech Stack:** Rust, `serde_json`, existing `buildc corpus verify`, semantic-corpus receipt fixtures, Cargo test slices.

## Global Constraints

- Receipt schema remains exactly `buildlang-lsp-dispatch-receipt/v0`.
- New observed fields are exactly `compiler_diagnostics` and `type_errors`.
- Compiler diagnostic source strings are exactly `buildlang/lexer`, `buildlang/parser`, and `buildlang/type-checker`.
- Remove `compiler type-checker diagnostics in LSP` only after checked receipt evidence exists.
- Keep `full VS Code extension readiness`.
- Do not claim pull diagnostics, full typed LSP deserialization, diagnostic latency, or VS Code end-to-end readiness.
- Keep files this slice controls at or below 300 lines; split `compiler/src/lsp_dispatch/fixture.rs` before adding another fixture.

---

### Task 1: Tag Compiler Diagnostics at the LSP Boundary

**Files:** Modify `compiler/src/lsp/diagnostics.rs`; modify `compiler/src/lsp/server.rs`.

**Interfaces:** Produces constants `LEXER_DIAGNOSTIC_SOURCE`, `PARSER_DIAGNOSTIC_SOURCE`, `TYPE_CHECKER_DIAGNOSTIC_SOURCE`; preserves `DiagnosticsProvider::compute(&self, doc: &Document) -> PublishDiagnosticsParams`.

- [ ] **Step 1: Write provider RED test.** Append to `compiler/src/lsp/diagnostics.rs` tests:

```rust
#[test]
fn compiler_type_error_diagnostic_uses_type_checker_source() {
    let documents = Arc::new(DocumentStore::new());
    let provider = DiagnosticsProvider::new(documents.clone());
    let doc = documents.open(TextDocumentItem { uri: "file:///workspace/type_error.bld".to_string(), language_id: "build".to_string(), version: 1, text: "const BAD: i32 = \"oops\";\nfn main() {}\n".to_string() });
    let published = provider.compute(&doc);
    assert!(published.diagnostics.iter().any(|d| d.source.as_deref() == Some("buildlang/type-checker") && d.message.contains("type mismatch")), "expected type-checker diagnostic in {:#?}", published.diagnostics);
}
```

- [ ] **Step 2: Write raw dispatch RED test.** Append to `compiler/src/lsp/server.rs` tests:

```rust
#[test]
fn raw_dispatch_did_open_returns_type_checker_diagnostic_source() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/type_error.bld","languageId":"build","version":1,"text":"const BAD: i32 = \"oops\";\nfn main() {}\n"}}}"#).expect("didOpen should publish diagnostics");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse diagnostics");
    let diagnostics = json["params"]["diagnostics"].as_array().expect("diagnostics array");
    assert!(diagnostics.iter().any(|d| d["source"] == "buildlang/type-checker" && d["message"].as_str().is_some_and(|m| m.contains("type mismatch"))), "expected type-checker diagnostic in {diagnostics:#?}");
}
```

- [ ] **Step 3: Verify RED.**

```powershell
cargo test --manifest-path compiler\Cargo.toml --lib compiler_type_error_diagnostic_uses_type_checker_source --quiet
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch_did_open_returns_type_checker_diagnostic_source --quiet
```

Expected: both fail because compiler diagnostics currently use source `buildlang`.

- [ ] **Step 4: Implement source tags.** In `diagnostics.rs`, add:

```rust
const LEXER_DIAGNOSTIC_SOURCE: &str = "buildlang/lexer";
const PARSER_DIAGNOSTIC_SOURCE: &str = "buildlang/parser";
const TYPE_CHECKER_DIAGNOSTIC_SOURCE: &str = "buildlang/type-checker";
```

Replace only the four compiler-pipeline `source: Some("buildlang".to_string())` assignments in `check_types` with lexer, parser, parser, and type-checker constants in that order. Leave heuristic diagnostics unchanged.

- [ ] **Step 5: Verify GREEN and commit.**

```powershell
cargo test --manifest-path compiler\Cargo.toml --lib diagnostics --quiet
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
git add compiler\src\lsp\diagnostics.rs compiler\src\lsp\server.rs
git commit -m "feat: tag lsp compiler diagnostics"
```

Expected: diagnostics and raw dispatch slices pass; commit succeeds.

---

### Task 2: Observe Compiler Diagnostics in LSP Dispatch Receipts

**Files:** Create `compiler/src/lsp_dispatch/observe.rs`; modify `compiler/src/lsp_dispatch.rs`; modify `compiler/src/lsp_dispatch/model.rs`; modify `compiler/src/lsp_dispatch/fixture.rs`; modify `compiler/src/lsp_dispatch/tests.rs`.

**Interfaces:** Produces `pub(super) fn observe_response(method: &str, response: Option<&serde_json::Value>) -> LspDispatchObserved`; adds `LspDispatchObserved.compiler_diagnostics: usize` and `LspDispatchObserved.type_errors: usize`.

- [ ] **Step 1: Write receipt RED test.** Append to `compiler/src/lsp_dispatch/tests.rs`:

```rust
#[test]
fn lsp_fixture_sequence_records_compiler_type_diagnostics() {
    let (_root, _manifest, receipt) = built_receipt();
    let fixture = receipt.fixtures.iter().find(|f| f.id == "did-change-type-error").expect("type-error fixture");
    assert_eq!(fixture.method, "textDocument/didChange");
    assert!(fixture.observed.compiler_diagnostics >= 1);
    assert!(fixture.observed.type_errors >= 1);
    assert!(!receipt.summary.known_gaps.contains(&"compiler type-checker diagnostics in LSP".to_string()));
    assert!(receipt.summary.known_gaps.contains(&"full VS Code extension readiness".to_string()));
}
```

- [ ] **Step 2: Verify RED.**

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_fixture_sequence_records_compiler_type_diagnostics --quiet
```

Expected: missing observed fields or missing `did-change-type-error`.

- [ ] **Step 3: Implement observed fields.** In `model.rs`, add after `diagnostics`:

```rust
#[serde(default, skip_serializing_if = "is_zero")]
pub(crate) compiler_diagnostics: usize,
#[serde(default, skip_serializing_if = "is_zero")]
pub(crate) type_errors: usize,
```

Use comma separators, matching the struct's existing field style.

- [ ] **Step 4: Split observation from `fixture.rs`.** Create `observe.rs` by moving `observe_response`, `result_array_len`, and `workspace_edit_count` out of `fixture.rs`; add compiler counts in the diagnostics branch:

```rust
const LEXER_SOURCE: &str = "buildlang/lexer";
const PARSER_SOURCE: &str = "buildlang/parser";
const TYPE_CHECKER_SOURCE: &str = "buildlang/type-checker";
observed.compiler_diagnostics = diagnostics.iter().filter(|d| matches!(d["source"].as_str(), Some(LEXER_SOURCE | PARSER_SOURCE | TYPE_CHECKER_SOURCE))).count();
observed.type_errors = diagnostics.iter().filter(|d| d["source"].as_str() == Some(TYPE_CHECKER_SOURCE)).count();
```

Initialize both new fields to `0`. In `lsp_dispatch.rs`, add `mod observe;`. In `fixture.rs`, add `use super::observe::observe_response;`.

- [ ] **Step 5: Add fixture and remove stale gap.** In `fixture_sequence()`, add after `rename` and before `shutdown`:

```rust
fixture("did-change-type-error", "textDocument/didChange", serde_json::json!({"textDocument": {"uri": uri, "version": 3}, "contentChanges": [{"text": "const BAD: i32 = \"oops\";\nfn main() {}\n"}]})),
```

In `summarize_fixtures`, set `known_gaps` to `["full VS Code extension readiness"]`.

- [ ] **Step 6: Verify GREEN and commit.**

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
(Get-Content compiler\src\lsp_dispatch\fixture.rs).Count
(Get-Content compiler\src\lsp_dispatch\observe.rs).Count
git add compiler\src\lsp_dispatch.rs compiler\src\lsp_dispatch\fixture.rs compiler\src\lsp_dispatch\model.rs compiler\src\lsp_dispatch\observe.rs compiler\src\lsp_dispatch\tests.rs
git commit -m "feat: observe lsp compiler diagnostics"
```

Expected: LSP dispatch tests pass; both counted files are at most 300 lines; commit succeeds.

---

### Task 3: Add CLI Drift Tests and Regenerate Receipt

**Files:** Modify `compiler/tests/cli.rs`; temporarily modify `compiler/src/lsp_dispatch/tests.rs`; modify `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`.

**Interfaces:** Produces tests `corpus_verify_rejects_lsp_dispatch_compiler_diagnostic_observed_drift` and `corpus_verify_rejects_lsp_dispatch_stale_compiler_diagnostics_gap`.

- [ ] **Step 1: Write CLI RED tests.** Append beside LSP dispatch drift tests in `compiler/tests/cli.rs`:

```rust
#[test]
fn corpus_verify_rejects_lsp_dispatch_compiler_diagnostic_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_compiler_diagnostic_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        let fixture = receipt["fixtures"].as_array_mut().expect("fixtures should be an array").iter_mut().find(|f| f["id"] == "did-change-type-error").expect("type-error fixture should exist");
        fixture["observed"]["compiler_diagnostics"] = serde_json::Value::from(0);
        receipt
    });
    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch fixture did-change-type-error observed drift");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_stale_compiler_diagnostics_gap() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_compiler_diagnostics_gap");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["known_gaps"] = serde_json::json!(["compiler type-checker diagnostics in LSP", "full VS Code extension readiness"]);
        receipt
    });
    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch summary drift");
}
```

- [ ] **Step 2: Verify RED.**

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
```

Expected: the new compiler-diagnostic drift test fails against the stale checked receipt because `did-change-type-error` is absent.

- [ ] **Step 3: Regenerate checked receipt.** Temporarily append ignored writer `write_semantic_corpus_lsp_dispatch_receipt` to `compiler/src/lsp_dispatch/tests.rs`:

```rust
#[test]
#[ignore]
fn write_semantic_corpus_lsp_dispatch_receipt() {
    let (root, _manifest, receipt) = built_receipt();
    let rendered = serde_json::to_string_pretty(&receipt).expect("render LSP dispatch receipt");
    std::fs::write(root.join("receipts").join(LSP_DISPATCH_RECEIPT), format!("{rendered}\n")).expect("write LSP dispatch receipt");
}
```

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin buildc write_semantic_corpus_lsp_dispatch_receipt -- --ignored --nocapture
```

Expected: writer passes and rewrites `semantic-corpus\receipts\lsp-dispatch-2026-06-18.json`. Remove the temporary writer immediately.

- [ ] **Step 4: Verify GREEN and commit.**

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git add compiler\tests\cli.rs semantic-corpus\receipts\lsp-dispatch-2026-06-18.json
git commit -m "test: verify lsp compiler diagnostic receipt"
```

Expected: tests pass; `corpus verify` prints `lsp dispatch receipt: ok`; commit succeeds.

---

### Task 4: Update Docs and Run Final Verification

**Files:** Modify `compiler/src/lsp/STATUS.md`; modify `docs/tutorial.md`.

**Interfaces:** Docs state compiler-backed diagnostics are raw-dispatch receipt verified; docs keep VS Code end-to-end readiness and typed LSP deserialization as incomplete.

- [ ] **Step 1: Update docs.** In `STATUS.md`, mention compiler-backed diagnostics in the Diagnostics and Dispatch receipt bullets; remove `No integration with the compiler's type checker for semantic diagnostics.` from Not Started; keep typed-request and VS Code limits. In `docs/tutorial.md`, add `compiler-backed diagnostics` to the verified raw dispatch LSP feature sentence and keep the VS Code warning.

- [ ] **Step 2: Run final verification.**

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib diagnostics --quiet
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git diff --check
```

Expected: all commands exit 0; `corpus verify` prints `lsp dispatch receipt: ok`.

- [ ] **Step 3: Pre-commit gates and commit.**

```powershell
git add compiler\src\lsp\STATUS.md docs\tutorial.md
git check-ignore .env .env.local .env.production
git diff --cached --check
$envFiles = git diff --cached --name-only | Select-String -Pattern '(^|/)\.env(\.|$)'; if ($envFiles) { $envFiles; exit 1 }; $diff = git diff --cached -U0; $pattern = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('KD9pKShhcGlbXy1dP2tleXxzZWNyZXR8dG9rZW58cGFzc3dvcmR8cHJpdmF0ZVtfLV0/a2V5fEJFR0lOIChSU0F8T1BFTlNTSHxFQ3xEU0EpPyA/UFJJVkFURSBLRVkp')); $hits = $diff | Select-String -Pattern $pattern; if ($hits) { $hits; exit 1 }; 'staged credential scan clean'
git commit -m "docs: record lsp compiler diagnostics evidence"
```

Expected: `.env`, `.env.local`, and `.env.production` are ignored; staged diff check exits 0; credential scan prints `staged credential scan clean`; commit succeeds.

---

## Completion Checklist

- `compiler/src/lsp_dispatch/fixture.rs` and `compiler/src/lsp_dispatch/observe.rs` are each at or below 300 lines.
- Checked receipt contains fixture id `did-change-type-error`.
- Checked receipt records nonzero `compiler_diagnostics` and `type_errors`.
- Checked receipt summary contains `full VS Code extension readiness`.
- Checked receipt summary does not contain `compiler type-checker diagnostics in LSP`.
- Final branch contains no `.env` files and no staged credential-pattern hits.
