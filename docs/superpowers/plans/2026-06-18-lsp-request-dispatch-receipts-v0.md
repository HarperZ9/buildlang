# LSP Request-Dispatch Receipts v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add checked LSP request-dispatch receipts that prove selected `buildc lsp` JSON-RPC request surfaces are reachable and structurally stable.

**Architecture:** Expose the existing raw LSP dispatcher through a small public function in `compiler/src/lsp/server.rs`, then add a focused `compiler/src/lsp_dispatch.rs` receipt builder/verifier in the `buildc` binary. The receipt replays deterministic raw request fixtures against a fresh `LanguageServer`, stores stable response digests plus observed counts, is referenced from the substrate receipt, and is verified by `buildc corpus verify`.

**Tech Stack:** Rust, `serde`, `serde_json`, `sha2`, existing `buildc` CLI test harness, semantic corpus JSON receipts.

## Global Constraints

- v0 proves raw LSP request dispatch evidence, not full VS Code extension readiness.
- v0 must not replace the simplified JSON extraction parser with a full JSON-RPC parser.
- v0 must not claim compiler type-checker backed LSP diagnostics.
- v0 must not add semantic tokens, code lens, workspace symbol, execute command, latency receipts, package public API receipts, registry download behavior, or cryptographic signing.
- The receipt schema is exactly `buildlang-lsp-dispatch-receipt/v0`.
- The canonical receipt path is exactly `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`.
- Successful corpus verification prints exactly `lsp dispatch receipt: ok`.
- Manual edits use `apply_patch`.
- Write failing tests before production code.
- Use targeted regression slices by default.

---

## File Structure

- Modify `compiler/src/lsp/server.rs`: expose `dispatch_raw_message(server: &mut LanguageServer, content: &str) -> Option<String>` and add raw dispatch unit tests.
- Create `compiler/src/lsp_dispatch.rs`: receipt model, fixture runner, digest projection, verifier, and unit tests.
- Modify `compiler/src/main.rs`: import `lsp_dispatch`, read and verify the receipt in `cmd_corpus_verify`, print `lsp dispatch receipt: ok`, and validate `SubstrateLspSurface`.
- Modify `compiler/tests/cli.rs`: add LSP dispatch receipt drift tests and substrate LSP receipt path tests.
- Create `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`: canonical checked receipt.
- Modify `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`: add `lsp_surface`.
- Modify `compiler/src/lsp/STATUS.md`: correct reachable dispatch scope and remaining limitations.
- Modify `docs/tutorial.md`: describe verified raw dispatch scope without claiming full VS Code readiness.

---

### Task 1: Raw LSP Dispatch Contract Tests

**Files:**
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: existing private `handle_raw_message(server: &mut LanguageServer, content: &str) -> Option<String>`.
- Produces: `pub fn dispatch_raw_message(server: &mut LanguageServer, content: &str) -> Option<String>`.

- [ ] **Step 1: Write failing raw dispatch tests**

Add these tests inside the existing `#[cfg(test)] mod tests` in `compiler/src/lsp/server.rs`:

```rust
#[test]
fn raw_dispatch_initialize_reports_core_capabilities() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///workspace"}}"#,
    )
    .expect("initialize should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse initialize response");

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["result"]["capabilities"]["hoverProvider"], true);
    assert_eq!(json["result"]["capabilities"]["definitionProvider"], true);
    assert_eq!(json["result"]["capabilities"]["referencesProvider"], true);
    assert_eq!(json["result"]["capabilities"]["documentSymbolProvider"], true);
}

#[test]
fn raw_dispatch_did_open_returns_diagnostics_notification() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.bld","languageId":"build","version":1,"text":"fn main() {\n    let x = 1;\n}\n"}}}"#,
    )
    .expect("didOpen should publish diagnostics");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse diagnostics notification");

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["method"], "textDocument/publishDiagnostics");
    assert_eq!(json["params"]["uri"], "file:///workspace/main.bld");
    assert!(json["params"]["diagnostics"].is_array());
}

#[test]
fn raw_dispatch_document_symbol_returns_opened_function() {
    let mut server = LanguageServer::new();
    dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.bld","languageId":"build","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#,
    )
    .expect("didOpen should publish diagnostics");

    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"file:///workspace/main.bld"}}}"#,
    )
    .expect("documentSymbol should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse documentSymbol response");
    let names = json["result"]
        .as_array()
        .expect("documentSymbol result array")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&"helper"), "expected helper symbol in {names:?}");
    assert!(names.contains(&"main"), "expected main symbol in {names:?}");
}
```

- [ ] **Step 2: Run tests and verify RED**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --lib raw_dispatch --quiet
```

Expected: compile failure because `dispatch_raw_message` does not exist.

- [ ] **Step 3: Add minimal dispatch wrapper**

Add below `handle_raw_message` or above it:

```rust
pub fn dispatch_raw_message(
    server: &mut LanguageServer,
    content: &str,
) -> Option<String> {
    handle_raw_message(server, content)
}
```

- [ ] **Step 4: Run tests and verify GREEN**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --lib raw_dispatch --quiet
```

Expected: the three raw dispatch tests pass.

---

### Task 2: LSP Dispatch Receipt Builder Unit Tests

**Files:**
- Create: `compiler/src/lsp_dispatch.rs`
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `buildlang::lsp::server::{dispatch_raw_message, LanguageServer}`.
- Produces:
  - `pub(crate) const LSP_DISPATCH_RECEIPT: &str = "lsp-dispatch-2026-06-18.json";`
  - `pub(crate) struct LspDispatchReceipt`
  - `pub(crate) fn build_lsp_dispatch_receipt(root: &Path, manifest: &SemanticCorpusManifest) -> Result<LspDispatchReceipt, String>`
  - `pub(crate) fn verify_lsp_dispatch_receipt(root: &Path, receipt: &LspDispatchReceipt, manifest: &SemanticCorpusManifest) -> Result<(), i32>`

- [ ] **Step 1: Add module declaration**

In `compiler/src/main.rs`, add:

```rust
mod lsp_dispatch;
use lsp_dispatch::{verify_lsp_dispatch_receipt, LspDispatchReceipt, LSP_DISPATCH_RECEIPT};
```

- [ ] **Step 2: Write failing receipt unit tests**

Create `compiler/src/lsp_dispatch.rs` with model structs and these tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_fixture_sequence_records_initialize_and_document_symbols() {
        let root = repo_semantic_corpus_root();
        let manifest = read_manifest(&root).expect("read semantic manifest");
        let receipt = build_lsp_dispatch_receipt(&root, &manifest).expect("build receipt");

        assert_eq!(receipt.schema, LSP_DISPATCH_SCHEMA);
        assert!(receipt.fixtures.iter().any(|fixture| fixture.method == "initialize"));
        let document_symbol = receipt
            .fixtures
            .iter()
            .find(|fixture| fixture.method == "textDocument/documentSymbol")
            .expect("documentSymbol fixture");
        assert!(document_symbol.observed.document_symbols >= 2);
    }

    #[test]
    fn lsp_fixture_summary_sorts_methods_and_response_kinds() {
        let receipt = build_lsp_dispatch_receipt(
            &repo_semantic_corpus_root(),
            &read_manifest(&repo_semantic_corpus_root()).expect("read semantic manifest"),
        )
        .expect("build receipt");

        assert!(receipt.summary.methods.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(receipt.summary.response_kinds.contains(&"response".to_string()));
        assert!(receipt.summary.response_kinds.contains(&"notification".to_string()));
        assert!(receipt.summary.response_kinds.contains(&"none".to_string()));
    }
}
```

Add helper functions in the test module:

```rust
fn repo_semantic_corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have repository parent")
        .join("semantic-corpus")
}

fn read_manifest(root: &Path) -> Result<SemanticCorpusManifest, String> {
    serde_json::from_slice(&std::fs::read(root.join("manifest.json")).map_err(|err| err.to_string())?)
        .map_err(|err| err.to_string())
}
```

- [ ] **Step 3: Run tests and verify RED**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --bin buildc lsp_dispatch --quiet
```

Expected: compile failures for missing receipt types and builder functions.

- [ ] **Step 4: Implement minimal receipt model and builder**

Implement structs:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchReceipt {
    pub(crate) schema: String,
    pub(crate) receipt_id: String,
    pub(crate) created_at: String,
    pub(crate) compiler: String,
    pub(crate) language: String,
    pub(crate) source_set: LspDispatchSourceSet,
    pub(crate) lsp_model: LspDispatchModel,
    pub(crate) fixtures: Vec<LspDispatchFixture>,
    pub(crate) summary: LspDispatchSummary,
}
```

Add companion structs for `LspDispatchSourceSet`, `LspDispatchModel`,
`LspDispatchFixture`, `LspDispatchObserved`, `LspDispatchDigest`, and
`LspDispatchSummary`.

Fixture projection rules:

- `response_kind` is `"response"` when the output has `result` or `error`.
- `response_kind` is `"notification"` when the output has `method` and no `id`.
- `response_kind` is `"none"` when dispatch returns `None`.
- `result_digest` is SHA-256 over normalized response JSON, or SHA-256 over
  the literal bytes `b"none"` for no response.
- `observed` counts are derived from parsed response JSON:
  - `diagnostics`: length of `params.diagnostics`
  - `completion_items`: length of `result.items`
  - `document_symbols`: length of `result`
  - `locations`: length of `result`
  - `text_edits`: length of `result`
  - `folding_ranges`: length of `result`
  - `has_result`: true when JSON has a `result` field

- [ ] **Step 5: Run tests and verify GREEN**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --bin buildc lsp_dispatch --quiet
```

Expected: receipt builder unit tests pass.

---

### Task 3: LSP Dispatch Receipt Verifier and Corpus Integration

**Files:**
- Modify: `compiler/src/lsp_dispatch.rs`
- Modify: `compiler/src/main.rs`
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: `build_lsp_dispatch_receipt`.
- Produces: `buildc corpus verify` validates LSP receipt and prints `lsp dispatch receipt: ok`.

- [ ] **Step 1: Write failing CLI tests**

In `compiler/tests/cli.rs`, add:

```rust
fn write_lsp_dispatch_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("lsp-dispatch-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read LSP dispatch receipt"))
            .expect("parse LSP dispatch receipt");
    let rendered = serde_json::to_string_pretty(&transform(receipt))
        .expect("render modified LSP dispatch receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified LSP dispatch receipt");
}
```

Add tests:

```rust
#[test]
fn corpus_verify_checks_lsp_dispatch_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping LSP dispatch receipt verification because no C backend is available");
        return;
    }
    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with LSP dispatch receipt");
    assert!(output.status.success(), "stdout:\n{}\nstderr:\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("lsp dispatch receipt: ok"));
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_schema_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_schema");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("buildlang-lsp-dispatch-receipt/v9".into());
        receipt
    });
    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch receipt has unsupported schema");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_fixture_digest_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_digest");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["fixtures"][0]["result_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });
    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch fixture initialize result_digest mismatch");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_observed_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_observed");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["fixtures"][0]["observed"]["completion_items"] = serde_json::Value::from(999);
        receipt
    });
    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch fixture initialize observed drift");
}

#[test]
fn corpus_verify_rejects_lsp_dispatch_summary_drift() {
    let corpus_root = temp_semantic_corpus("lsp_dispatch_summary");
    write_lsp_dispatch_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["fixture_count"] = serde_json::Value::from(999);
        receipt
    });
    assert_corpus_verify_rejects(&corpus_root, "lsp dispatch summary drift");
}
```

- [ ] **Step 2: Run tests and verify RED**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture
```

Expected: tests fail because the receipt file and corpus verifier integration are missing.

- [ ] **Step 3: Implement verifier comparison**

In `compiler/src/lsp_dispatch.rs`, add:

```rust
pub(crate) fn verify_lsp_dispatch_receipt(
    root: &Path,
    receipt: &LspDispatchReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_lsp_dispatch_receipt(root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}
```

`validate_lsp_dispatch_receipt` should validate schema/compiler/language/source_set,
validate `source_set.manifest` points at `manifest.json`, then compare against
`build_lsp_dispatch_receipt(root, manifest)?` and return specific drift messages.

- [ ] **Step 4: Integrate with `cmd_corpus_verify`**

In `compiler/src/main.rs`, read and verify the receipt:

```rust
let lsp_receipt_path = receipts_dir.join(LSP_DISPATCH_RECEIPT);
let lsp_receipt: LspDispatchReceipt = read_json(&lsp_receipt_path)?;
verify_lsp_dispatch_receipt(&corpus_root, &lsp_receipt, &manifest)?;
```

Print after symbol graph:

```rust
println!("lsp dispatch receipt: ok");
```

---

### Task 4: Substrate LSP Surface and Canonical Receipt

**Files:**
- Modify: `compiler/src/main.rs`
- Modify: `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`
- Create: `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: `LSP_DISPATCH_RECEIPT`.
- Produces: substrate validation for `lsp_surface.lsp_receipt`.

- [ ] **Step 1: Add substrate struct and validation**

In `main.rs`, add `lsp_surface: SubstrateLspSurface` to `SubstrateReceipt` and define:

```rust
#[derive(serde::Deserialize)]
struct SubstrateLspSurface {
    protocol: String,
    dispatch: String,
    request_parser: String,
    lsp_receipt: String,
    #[serde(default)]
    known_gaps: Vec<String>,
}
```

Validate exact values:

```rust
protocol == "LSP JSON-RPC over stdio"
dispatch == "buildc lsp raw message dispatch"
request_parser == "simplified string extraction"
lsp_receipt == format!("receipts/{LSP_DISPATCH_RECEIPT}")
known_gaps is not empty
```

Use `validate_substrate_path(corpus_root, &receipt.lsp_surface.lsp_receipt, "lsp_surface.lsp_receipt")`.

- [ ] **Step 2: Add substrate drift tests**

Add tests:

```rust
#[test]
fn corpus_verify_rejects_substrate_lsp_receipt_missing_path() {
    let corpus_root = temp_semantic_corpus("substrate_lsp_path_missing");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["lsp_surface"]["lsp_receipt"] =
            serde_json::Value::String("receipts/missing-lsp-dispatch.json".into());
        receipt
    });
    assert_corpus_verify_rejects(
        &corpus_root,
        "substrate lsp_surface.lsp_receipt path not found",
    );
}

#[test]
fn corpus_verify_rejects_substrate_lsp_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_lsp_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["lsp_surface"]["lsp_receipt"] =
            serde_json::Value::String("../lsp-dispatch-2026-06-18.json".into());
        receipt
    });
    assert_corpus_verify_rejects(
        &corpus_root,
        "substrate lsp_surface.lsp_receipt must stay within corpus root",
    );
}
```

- [ ] **Step 3: Generate canonical receipt**

Add this temporary ignored test, run it once, and remove it before committing:

```rust
#[test]
#[ignore]
fn write_semantic_corpus_lsp_dispatch_receipt() {
    let root = repo_semantic_corpus_root();
    let manifest = read_manifest(&root).expect("read manifest");
    let receipt = build_lsp_dispatch_receipt(&root, &manifest).expect("build receipt");
    let rendered = serde_json::to_string_pretty(&receipt).expect("render receipt");
    std::fs::write(root.join("receipts").join(LSP_DISPATCH_RECEIPT), format!("{rendered}\n"))
        .expect("write receipt");
}
```

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --bin buildc write_semantic_corpus_lsp_dispatch_receipt -- --ignored --nocapture
```

Remove the temporary ignored test immediately after generating the JSON.

- [ ] **Step 4: Update substrate JSON**

Insert after `symbol_surface`:

```json
"lsp_surface": {
  "protocol": "LSP JSON-RPC over stdio",
  "dispatch": "buildc lsp raw message dispatch",
  "request_parser": "simplified string extraction",
  "lsp_receipt": "receipts/lsp-dispatch-2026-06-18.json",
  "known_gaps": [
    "compiler type-checker diagnostics in LSP",
    "full JSON-RPC deserialization",
    "full VS Code extension readiness"
  ]
}
```

---

### Task 5: Documentation Updates

**Files:**
- Modify: `compiler/src/lsp/STATUS.md`
- Modify: `docs/tutorial.md`

**Interfaces:**
- Consumes: implemented LSP dispatch receipt and current `server.rs`.
- Produces: docs that match verified dispatch scope.

- [ ] **Step 1: Update `compiler/src/lsp/STATUS.md`**

Replace the stale partial-runner statement with text saying:

```markdown
## Checked Evidence
- `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json` verifies a deterministic raw JSON-RPC fixture sequence through the same dispatch function used by `buildc lsp`.

## Partial
- The raw dispatch loop reaches lifecycle, text document sync, completion, hover, definition, references, document symbols, formatting, and folding requests.
- Request parsing still uses simplified string extraction, not full JSON-RPC deserialization.
- Diagnostics are provider/text-pattern based, not compiler type-checker diagnostics.
```

- [ ] **Step 2: Update `docs/tutorial.md`**

Replace the LSP feature wording with:

```markdown
- **LSP launch support** -- `buildc lsp` starts the current server loop. The
  raw dispatch path is receipt-checked for selected lifecycle, document sync,
  completion, hover, definition, references, document symbol, formatting, and
  folding requests. It still uses simplified request extraction and should not
  be treated as a full VS Code production language-server experience yet.
```

---

### Task 6: Verification and Commits

**Files:**
- All changed files.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: verified commits on a feature branch, then mergeable main.

- [ ] **Step 1: Run targeted verification**

Run:

```text
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo test --manifest-path compiler/Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler/Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus
git diff --check
```

- [ ] **Step 2: Run credential and `.env` gates**

Run:

```text
git check-ignore .env .env.local .env.production
```

Run a staged added-line credential scan using the current repo’s usual pattern:

```text
$added = git diff --cached --unified=0 | Where-Object { $_ -match '^\+' -and $_ -notmatch '^\+\+\+' }
$pattern = '(?i)(' + 'api[_-]' + 'key|' + 'tok' + 'en=' + '|pass' + 'word=' + '|BEGIN (RSA|OPENSSH|PRIVATE)' + '|AK' + 'IA[0-9A-Z]{16}' + '|xox[baprs]-' + '|sk-' + '[A-Za-z0-9]{20,})'
$matches = $added | Select-String -Pattern $pattern
if ($matches) { $matches | ForEach-Object { $_.Line }; exit 1 }
```

- [ ] **Step 3: Commit in focused chunks**

Use these commit shapes if the diff splits cleanly:

```text
git add compiler/src/lsp/server.rs
git commit -m "test: expose lsp raw dispatch"

git add compiler/src/lsp_dispatch.rs compiler/src/main.rs compiler/tests/cli.rs
git commit -m "feat: verify lsp dispatch receipts"

git add semantic-corpus/receipts/lsp-dispatch-2026-06-18.json semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json compiler/src/lsp/STATUS.md docs/tutorial.md
git commit -m "docs: record lsp dispatch receipt"
```

## Self-Review

- Spec coverage: raw dispatch evidence, receipt builder/verifier, substrate reference, canonical receipt, docs, tests, and non-goals are mapped to tasks.
- Placeholder scan: no placeholder work items are present.
- Type consistency: planned exported names match `main.rs` imports and the canonical receipt filename matches the design.
