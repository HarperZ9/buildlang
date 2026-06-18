# Typed Raw LSP Param Decoding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add typed raw-boundary LSP param decoding for currently dispatched methods so malformed params produce JSON-RPC `-32602` instead of silent defaults.

**Architecture:** Keep `server.rs` responsible for routing and response JSON. Add `compiler/src/lsp/raw_params.rs` to convert `JsonRpcMessage` into existing typed LSP structs from `message.rs` and `types.rs`. Valid-request behavior and the semantic-corpus LSP dispatch receipt remain stable.

**Tech Stack:** Rust 2021, `serde_json::Value`, existing QuantaLang LSP structs, Cargo test slices.

## Global Constraints

- Do not rewrite the server router into a method enum.
- Do not derive `serde::Deserialize` across the full LSP type tree.
- Do not add new LSP features.
- Do not modify provider behavior for completion, hover, symbols, diagnostics, code actions, formatting, rename, or folding ranges.
- Do not claim end-to-end VS Code extension verification.
- Preserve the valid LSP dispatch receipt unless a justified valid-output change is intentionally made.
- Numeric LSP positions must reject negative values before casting to `u32`.

---

### Task 1: Add RED Invalid-Param Raw Dispatch Tests

**Files:**
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: `dispatch_raw_message(server: &mut LanguageServer, content: &str) -> Option<String>`
- Produces: Four failing `raw_dispatch_invalid_params_*` tests that require JSON-RPC `-32602`

- [ ] **Step 1: Add a test helper and four failing tests**

Add this inside `mod tests` in `compiler/src/lsp/server.rs`, after `raw_dispatch_malformed_json_request_returns_parse_error`:

```rust
fn assert_invalid_params(response: Option<String>, expected_detail: &str) {
    let response = response.expect("invalid params should return an error response");
    let json: serde_json::Value =
        serde_json::from_str(&response).expect("parse invalid params response");
    assert_eq!(json["error"]["code"], -32602);
    let message = json["error"]["message"].as_str().expect("message");
    assert!(
        message.contains(expected_detail),
        "expected '{expected_detail}' in '{message}'"
    );
}

#[test]
fn raw_dispatch_invalid_params_did_open_missing_uri_returns_error() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","id":20,"method":"textDocument/didOpen","params":{"textDocument":{"languageId":"quanta","version":1,"text":"fn main() {}\n"}}}"#,
    );
    assert_invalid_params(response, "params.textDocument.uri is required");
}

#[test]
fn raw_dispatch_invalid_params_hover_negative_position_returns_error() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","id":21,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///workspace/main.quanta"},"position":{"line":-1,"character":0}}}"#,
    );
    assert_invalid_params(response, "params.position.line must be a non-negative integer");
}

#[test]
fn raw_dispatch_invalid_params_rename_missing_new_name_returns_error() {
    let mut server = LanguageServer::new();
    dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#,
    )
    .expect("didOpen should publish diagnostics");
    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","id":22,"method":"textDocument/rename","params":{"textDocument":{"uri":"file:///workspace/main.quanta"},"position":{"line":1,"character":14}}}"#,
    );
    assert_invalid_params(response, "params.newName is required");
}

#[test]
fn raw_dispatch_invalid_params_code_action_missing_context_returns_error() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","id":23,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"file:///workspace/main.quanta"},"range":{"start":{"line":1,"character":13},"end":{"line":1,"character":13}}}}"#,
    );
    assert_invalid_params(response, "params.context is required");
}
```

- [ ] **Step 2: Run the RED test slice**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch_invalid_params -- --nocapture
```

Expected: all four new tests fail because current dispatch returns normal results or notifications instead of `-32602`.

---

### Task 2: Add Typed Raw Param Decoder and Wire Server Dispatch

**Files:**
- Modify: `compiler/src/lsp/mod.rs`
- Modify: `compiler/src/lsp/jsonrpc.rs`
- Create: `compiler/src/lsp/raw_params.rs`
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: `JsonRpcMessage`
- Produces: `raw_params::decode_*(&JsonRpcMessage) -> Result<T, RawParamError>`
- Produces: `build_invalid_params_response(id: String, error: &raw_params::RawParamError) -> String`

- [ ] **Step 1: Expose read-only JSON value access**

In `compiler/src/lsp/jsonrpc.rs`, add this method in `impl JsonRpcMessage` beside `method()`:

```rust
pub(crate) fn value_at(&self, path: &[&str]) -> Option<&Value> {
    self.at(path)
}
```

- [ ] **Step 2: Register the new decoder module**

In `compiler/src/lsp/mod.rs`, add:

```rust
pub mod raw_params;
```

- [ ] **Step 3: Create `compiler/src/lsp/raw_params.rs`**

Create a focused module with `RawParamError` and implemented `decode_*` functions:

```rust
use serde_json::Value;

use super::jsonrpc::JsonRpcMessage;
use super::message::*;
use super::types::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawParamError {
    detail: String,
}

impl RawParamError {
    fn required(path: &'static str) -> Self { Self { detail: format!("{path} is required") } }
    fn non_negative_integer(path: &'static str) -> Self {
        Self { detail: format!("{path} must be a non-negative integer") }
    }
    pub fn detail(&self) -> &str { &self.detail }
}
```

Implement these functions with real bodies: `decode_initialize`, `decode_did_open`, `decode_did_change`, `decode_did_save`, `decode_did_close`, `decode_completion`, `decode_text_document_position`, `decode_document_uri`, `decode_code_action`, `decode_formatting`, and `decode_rename`. Required paths are `params.textDocument.uri`, `params.textDocument.languageId`, `params.textDocument.version`, `params.textDocument.text`, `params.contentChanges[0].text`, `params.position.line`, `params.position.character`, `params.range.start.*`, `params.range.end.*`, `params.context`, and `params.newName`.

Use `ClientCapabilities::default()`, `FormattingOptions::default()`, `CompletionParams { context: None }`, and `DidSaveTextDocumentParams { text: None }` as allowed defaults. Use existing `JsonRpcMessage` diagnostic helpers only after proving `params.context` exists.

- [ ] **Step 4: Wire invalid-param responses in `server.rs`**

Add:

```rust
use super::raw_params;
```

Add beside `build_error_response`:

```rust
fn build_invalid_params_response(id: String, error: &raw_params::RawParamError) -> String {
    build_error_response(id, -32602, &format!("Invalid params: {}", error.detail()))
}
```

Replace inline param construction in `initialize`, `didOpen`, `didChange`, `didSave`, `didClose`, `completion`, `hover`, `definition`, `references`, `documentSymbol`, `codeAction`, `formatting`, `rename`, and `foldingRange`.

For notifications, use:

```rust
let params = match raw_params::decode_did_open(&message) {
    Ok(params) => params,
    Err(error) => return id.map(|id| build_invalid_params_response(id, &error)),
};
```

For requests, use:

```rust
let response_id = id.unwrap_or_else(|| "1".to_string());
let params = match raw_params::decode_rename(&message) {
    Ok(params) => params,
    Err(error) => return Some(build_invalid_params_response(response_id, &error)),
};
```

- [ ] **Step 5: Run the GREEN raw dispatch slice**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch -- --nocapture
```

Expected: all raw dispatch tests pass, including the four invalid-param tests.

- [ ] **Step 6: Commit decoder integration**

Run:

```powershell
git add compiler\src\lsp\mod.rs compiler\src\lsp\jsonrpc.rs compiler\src\lsp\raw_params.rs compiler\src\lsp\server.rs
git diff --cached --check
git commit -m "feat: decode raw lsp params through typed helpers"
```

---

### Task 3: Preserve Receipt and Update LSP Status

**Files:**
- Modify: `compiler/src/lsp/STATUS.md`

**Interfaces:**
- Consumes: Task 2 decoder behavior
- Produces: status text that reflects typed raw-boundary helper coverage

- [ ] **Step 1: Verify valid receipt behavior stayed stable**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin quantac lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

Expected output includes `8 passed`, `11 passed`, and `lsp dispatch receipt: ok`.

- [ ] **Step 2: Update `compiler/src/lsp/STATUS.md`**

Replace the partial runner sentence about lightweight accessors with:

```markdown
Params for the currently dispatched raw methods decode through focused typed raw-boundary helpers before reaching the typed server methods.
```

Keep VS Code integration and full serde-backed request coverage as remaining limitations.

- [ ] **Step 3: Commit status update**

Run:

```powershell
git diff --check
cargo test --manifest-path compiler\Cargo.toml --bin quantac lsp_dispatch --quiet
git add compiler\src\lsp\STATUS.md
git diff --cached --check
git commit -m "docs: record typed raw lsp param decoding"
```

---

### Task 4: Final Verification and Push-Readiness

- [ ] **Step 1: Run final focused verification**

Run:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin quantac lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git check-ignore .env .env.local .env.production
git diff --check
```

Expected: tests pass; `corpus verify` prints `lsp dispatch receipt: ok`; `.env`, `.env.local`, and `.env.production` are listed; diff check is clean.

- [ ] **Step 2: Report final state**

```powershell
git status --short --branch
git log --oneline --decorate -8
```

## Self-Review

- Spec coverage: Tasks cover invalid-param tests, typed decoder module, server integration, receipt preservation, LSP status docs, and final hygiene.
- Open-marker scan: clean.
- Type consistency: decoder names use `decode_*(&JsonRpcMessage) -> Result<T, RawParamError>` consistently; server integration uses the same names and existing LSP structs from `message.rs` and `types.rs`.
