# LSP Workspace Symbols v0 Implementation Plan
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire existing workspace symbol search into raw `buildc lsp` dispatch and receipt-verify it.

**Architecture:** Keep the change inside the current raw LSP path: decode `workspace/symbol` params in `raw_params.rs`, delegate to the existing `SymbolProvider::workspace_symbols`, serialize flat `SymbolInformation[]`, and extend the LSP dispatch receipt observer. No new symbol provider or global compiler index is introduced.

**Tech Stack:** Rust 2021, existing `buildlang::lsp` modules, `serde_json`, semantic-corpus LSP receipt verifier, Cargo test slices.

## Global Constraints

- Do not build a new compiler-backed workspace index.
- Do not scan unopened files from disk.
- Do not add workspace folders, project graph loading, or package registry integration.
- Do not change document symbol extraction behavior.
- Do not claim full VS Code extension readiness.
- Do not claim resolved type/effect/module identities for workspace symbols.
- Keep new/touched plan-controlled files at or below 300 lines; existing large router/test files may receive focused additions only.

---

## File Map

- Modify `compiler/src/lsp/raw_params.rs`: add `decode_workspace_symbol_query`.
- Modify `compiler/src/lsp/response_json.rs`: serialize `SymbolInformation[]`.
- Modify `compiler/src/lsp/server.rs`: add `LanguageServer::workspace_symbol`, initialize capability JSON, raw route, and raw dispatch tests.
- Modify `compiler/src/lsp_dispatch/{model.rs,observe.rs,fixture.rs,tests.rs}`, `compiler/tests/cli.rs`, and `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`: receipt evidence.
- Modify `STATUS.md`, `compiler/src/lsp/STATUS.md`, and `docs/tutorial.md`: status updates.

---

### Task 1: Raw Workspace Symbol Dispatch

**Files:**
- Modify: `compiler/src/lsp/raw_params.rs`
- Modify: `compiler/src/lsp/response_json.rs`
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: `JsonRpcMessage`, `SymbolProvider::workspace_symbols(&self, query: &str) -> Vec<SymbolInformation>`.
- Produces: `raw_params::decode_workspace_symbol_query(&JsonRpcMessage) -> Result<String, RawParamError>`, `LanguageServer::workspace_symbol(&self, query: &str) -> Vec<SymbolInformation>`, and `response_json::build_symbol_information_json(&[SymbolInformation]) -> String`.

- [ ] **Step 1: Write failing raw dispatch tests**

In `compiler/src/lsp/server.rs`, add tests:

```rust
#[test]
fn raw_dispatch_initialize_reports_workspace_symbol_capability() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///workspace"}}"#,
    ).expect("initialize should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse initialize response");
    assert_eq!(json["result"]["capabilities"]["workspaceSymbolProvider"], true);
}

#[test]
fn raw_dispatch_workspace_symbol_returns_opened_symbol() {
    let mut server = LanguageServer::new();
    dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.bld","languageId":"build","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#).expect("didOpen should publish diagnostics");
    let response = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","id":30,"method":"workspace/symbol","params":{"query":"help"}}"#).expect("workspace/symbol should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse workspace/symbol response");
    let symbols = json["result"].as_array().expect("workspace symbol result array");
    assert!(symbols.iter().any(|symbol| symbol["name"] == "helper" && symbol["location"]["uri"] == "file:///workspace/main.bld"));
}

#[test]
fn raw_dispatch_workspace_symbol_unmatched_query_returns_empty_array() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","id":31,"method":"workspace/symbol","params":{"query":"missing"}}"#).expect("workspace/symbol should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse workspace/symbol response");
    assert_eq!(json["result"].as_array().expect("result array").len(), 0);
}

#[test]
fn raw_dispatch_invalid_params_workspace_symbol_missing_query_returns_error() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","id":32,"method":"workspace/symbol","params":{}}"#);
    assert_invalid_params(response, "params.query is required");
}
```

- [ ] **Step 2: Run RED raw dispatch slice**

Run: `cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch -- --nocapture`
Expected: FAIL because `workspaceSymbolProvider` is absent from initialize JSON and `workspace/symbol` returns method-not-found.

- [ ] **Step 3: Implement raw param helper**

In `compiler/src/lsp/raw_params.rs`, add:

```rust
pub fn decode_workspace_symbol_query(message: &JsonRpcMessage) -> Result<String, RawParamError> {
    required_string(message, &["params", "query"], "params.query")
}
```

- [ ] **Step 4: Implement response serializer**

In `compiler/src/lsp/response_json.rs`, import `SymbolInformation` and add:

```rust
pub fn build_symbol_information_json(symbols: &[SymbolInformation]) -> String {
    serde_json::to_string(&symbols.iter().map(symbol_information_json).collect::<Vec<Value>>())
        .expect("serialize workspace symbols")
}
```

Add helpers that emit `name`, `kind`, `location`, optional `tags`, and optional `containerName`. Reuse existing `range_json` for `location.range`.

- [ ] **Step 5: Implement server method, capability JSON, and route**

In `server.rs`, add:

```rust
pub fn workspace_symbol(&self, query: &str) -> Vec<SymbolInformation> {
    self.symbols.workspace_symbols(query)
}
```

Add `.field_bool("workspaceSymbolProvider", true)` to `build_initialize_result`. Add a raw branch:

```rust
if method == "workspace/symbol" {
    let response_id = id.unwrap_or_else(|| "1".to_string());
    let query = match raw_params::decode_workspace_symbol_query(&message) {
        Ok(query) => query,
        Err(error) => return Some(build_invalid_params_response(response_id, &error)),
    };
    let symbols = server.workspace_symbol(&query);
    return Some(build_response(response_id, response_json::build_symbol_information_json(&symbols)));
}
```

- [ ] **Step 6: Run GREEN raw dispatch slice and commit**

Run: `cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch -- --nocapture`
Expected: PASS.

```powershell
git add compiler\src\lsp\raw_params.rs compiler\src\lsp\response_json.rs compiler\src\lsp\server.rs
git commit -m "feat: dispatch lsp workspace symbols"
```

---

### Task 2: Receipt Coverage

**Files:**
- Modify: `compiler/src/lsp_dispatch/model.rs`, `compiler/src/lsp_dispatch/observe.rs`, `compiler/src/lsp_dispatch/fixture.rs`, `compiler/src/lsp_dispatch/tests.rs`, `compiler/tests/cli.rs`, `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`

**Interfaces:**
- Produces: `LspDispatchObserved.workspace_symbols: usize`.

- [ ] **Step 1: Write failing receipt tests**

Add a unit test in `compiler/src/lsp_dispatch/tests.rs`:

```rust
#[test]
fn lsp_fixture_sequence_records_workspace_symbols() {
    let (_root, _manifest, receipt) = built_receipt();
    let fixture = receipt.fixtures.iter().find(|fixture| fixture.id == "workspace-symbol").expect("workspace symbol fixture");
    assert_eq!(fixture.method, "workspace/symbol");
    assert!(fixture.observed.workspace_symbols > 0);
}
```

Add a CLI drift test in `compiler/tests/cli.rs` that finds fixture id `"workspace-symbol"`, sets `observed.workspace_symbols` to `0`, and expects `lsp dispatch fixture workspace-symbol observed drift`.

- [ ] **Step 2: Run RED receipt slice**

Run: `cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch -- --nocapture`
Expected: FAIL because `workspace_symbols` is not modeled or observed.

- [ ] **Step 3: Model and observe workspace symbols**

Add to `LspDispatchObserved`:

```rust
#[serde(default, skip_serializing_if = "is_zero")]
pub(crate) workspace_symbols: usize,
```

Initialize it to `0` and observe:

```rust
"workspace/symbol" => observed.workspace_symbols = result_array_len(value),
```

- [ ] **Step 4: Add fixture and refresh receipt**

In `fixture_sequence()`, insert after `did-open`:

```rust
request_fixture(13, "workspace-symbol", "workspace/symbol", serde_json::json!({"query": "help"})),
```

Temporarily add an ignored writer in `compiler/src/lsp_dispatch/tests.rs` that writes `build_lsp_dispatch_receipt` as pretty JSON to `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`. Run it, then remove the writer before committing.

Run: `cargo test --manifest-path compiler\Cargo.toml --bin buildc write_semantic_corpus_lsp_dispatch_receipt -- --ignored --nocapture`
Expected: PASS and the receipt JSON changes.

- [ ] **Step 5: Run GREEN receipt slices and commit**

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git add compiler\src\lsp_dispatch compiler\tests\cli.rs semantic-corpus\receipts\lsp-dispatch-2026-06-18.json
git commit -m "test: verify lsp workspace symbol receipt"
```

Expected: all PASS; `corpus verify` prints `lsp dispatch receipt: ok`.

---

### Task 3: Documentation and Final Verification

**Files:**
- Modify: `compiler/src/lsp/STATUS.md`, `STATUS.md`, `docs/tutorial.md`

**Interfaces:**
- Consumes: verified workspace-symbol dispatch and receipt evidence.
- Produces: accurate public status text.

- [ ] **Step 1: Update docs**

Document that `workspace/symbol` is receipt-verified for opened documents. Keep full compiler-backed global indexing and end-to-end VS Code behavior as open gaps.

- [ ] **Step 2: Run final targeted verification**

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

Expected: all PASS.

- [ ] **Step 3: Commit docs and run hygiene**

```powershell
git add STATUS.md compiler\src\lsp\STATUS.md docs\tutorial.md
git commit -m "docs: record lsp workspace symbol support"
git check-ignore .env .env.local .env.production
git diff --check origin/main..HEAD
git diff --name-only origin/main..HEAD
```

Expected: `.env`, `.env.local`, and `.env.production` are ignored; diff check has no output; changed file list has no secrets or `.env` files.
