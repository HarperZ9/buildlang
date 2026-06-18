# LSP JSON-RPC Parser v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace fragile LSP substring dispatch with structural JSON-RPC parsing while preserving current LSP behavior and receipt verification.

**Architecture:** Add a focused `compiler/src/lsp/jsonrpc.rs` parser around `serde_json::Value`, then route `server.rs` dispatch through the parsed method and params. Update the LSP dispatch receipt and docs to claim the stronger parser only after tests and corpus verification prove it.

**Tech Stack:** Rust, `serde_json`, existing LSP server/types/builders, semantic-corpus receipts.

## Global Constraints

- Keep new source files under 300 lines.
- No new dependencies; `serde_json` already exists in `compiler/Cargo.toml`.
- Preserve existing LSP response JSON builders and provider behavior.
- Use TDD: each production change must follow a failing focused test.
- Receipt metadata must not overclaim full VS Code readiness or type-checker diagnostics.

---

### Task 1: Raw Dispatch Parser Tests

**Files:**
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: `dispatch_raw_message(server: &mut LanguageServer, content: &str) -> Option<String>`
- Produces: failing tests that require structural JSON parsing.

- [ ] **Step 1: Add failing tests**

Add tests in the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn raw_dispatch_initialize_accepts_pretty_json_and_string_id() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(
        &mut server,
        r#"{
          "jsonrpc": "2.0",
          "params": { "rootUri": "file:///workspace" },
          "method": "initialize",
          "id": "init-1"
        }"#,
    )
    .expect("initialize should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse response");
    assert_eq!(json["id"], "init-1");
    assert_eq!(json["result"]["capabilities"]["hoverProvider"], true);
}

#[test]
fn raw_dispatch_document_symbol_accepts_reordered_pretty_json() {
    let mut server = LanguageServer::new();
    dispatch_raw_message(
        &mut server,
        r#"{
          "params": {
            "textDocument": {
              "text": "fn helper() -> i32 { 1 }\nfn main() { helper(); }\n",
              "version": 1,
              "languageId": "quanta",
              "uri": "file:///workspace/main.quanta"
            }
          },
          "method": "textDocument/didOpen",
          "jsonrpc": "2.0"
        }"#,
    )
    .expect("didOpen should publish diagnostics");
    let response = dispatch_raw_message(
        &mut server,
        r#"{
          "method": "textDocument/documentSymbol",
          "params": { "textDocument": { "uri": "file:///workspace/main.quanta" } },
          "id": 2,
          "jsonrpc": "2.0"
        }"#,
    )
    .expect("documentSymbol should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse response");
    let names = json["result"]
        .as_array()
        .expect("documentSymbol result array")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"main"));
}

#[test]
fn raw_dispatch_malformed_json_request_returns_parse_error() {
    let mut server = LanguageServer::new();
    let response = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","id":9,"method":"initialize""#)
        .expect("malformed request with id should return an error response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse response");
    assert_eq!(json["id"], 9);
    assert_eq!(json["error"]["code"], -32700);
}
```

- [ ] **Step 2: Run RED**

Run: `cargo test --manifest-path compiler/Cargo.toml --lib json_rpc --quiet`

Expected: at least the pretty/reordered JSON tests fail because the current dispatcher only matches compact method substrings.

### Task 2: Parser Module and Dispatch Wiring

**Files:**
- Create: `compiler/src/lsp/jsonrpc.rs`
- Modify: `compiler/src/lsp/mod.rs`
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Produces: `JsonRpcMessage::parse(content: &str) -> Result<JsonRpcMessage, JsonRpcParseError>`
- Produces: accessors for `id_json()`, `method()`, `string_at(&[&str])`, `i64_at(&[&str])`, `uri()`, `position()`, and `content_change_text()`.

- [ ] **Step 1: Implement minimal parser/accessors**

Use `serde_json::Value`; serialize the `id` field back to compact JSON with `serde_json::to_string`.

- [ ] **Step 2: Wire `handle_raw_message` through parsed method**

Parse once at the top, return `-32700` when parsing fails and a recoverable ID exists, then replace method `contains` checks and extraction helpers with parsed accessors.

- [ ] **Step 3: Run GREEN**

Run: `cargo test --manifest-path compiler/Cargo.toml --lib json_rpc --quiet`

Expected: new raw dispatch JSON-RPC parser tests pass.

### Task 3: Receipt and Corpus Verification

**Files:**
- Modify: `compiler/src/lsp_dispatch.rs`
- Modify: `compiler/src/lsp_dispatch/fixture.rs`
- Modify: `compiler/tests/cli.rs`
- Modify: `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`
- Modify: `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`

**Interfaces:**
- Consumes: `build_lsp_dispatch_receipt`
- Produces: receipt metadata with `request_parser = "serde_json structural JSON-RPC parser"`

- [ ] **Step 1: Add receipt drift tests**

Extend the LSP dispatch CLI tests to reject stale `lsp_model.request_parser` and stale known gap `full JSON-RPC deserialization`.

- [ ] **Step 2: Run RED**

Run: `cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture`

Expected: tests fail until receipt builder and canonical receipt metadata are updated.

- [ ] **Step 3: Update builder and fixtures**

Update `lsp_model.request_parser`, remove the obsolete known gap, and make at least one fixture payload pretty/reordered so the receipt exercises the parser improvement.

- [ ] **Step 4: Regenerate canonical receipt**

Use a temporary ignored writer test or another local-only helper, then remove the helper immediately.

- [ ] **Step 5: Run GREEN**

Run: `cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture`

Expected: all LSP dispatch CLI tests pass.

### Task 4: Docs, Verification, Commit, Merge

**Files:**
- Modify: `compiler/src/lsp/STATUS.md`
- Modify: `docs/tutorial.md`

**Interfaces:**
- Produces: docs that accurately describe structural JSON-RPC parsing and remaining gaps.

- [ ] **Step 1: Update docs**

Replace simplified-parser language with structural parser language. Keep VS Code readiness and type-checker diagnostics as remaining gaps.

- [ ] **Step 2: Run final targeted verification**

Run:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib json_rpc --quiet
cargo test --manifest-path compiler\Cargo.toml --bin quantac lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git diff --check
```

- [ ] **Step 3: Commit**

Before commit, verify `.env`, `.env.local`, and `.env.production` are ignored, run `git diff --cached --check`, and scan staged additions for credential-like names.

Commit message: `feat: parse lsp json-rpc structurally`
