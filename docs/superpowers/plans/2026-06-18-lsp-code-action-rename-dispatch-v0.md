# LSP Code Action + Rename Dispatch v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire existing LSP code action and rename providers into raw JSON-RPC dispatch and receipt-verify those methods in the semantic corpus.

**Architecture:** Extend the structural JSON-RPC parser with accessors for code-action and rename params, add a focused response serializer module for `CodeAction` and `WorkspaceEdit`, then add raw dispatch branches in `server.rs`. Extend the LSP dispatch receipt with observed counts for `code_actions` and `workspace_edits`.

**Tech Stack:** Rust, `serde_json`, existing `quantalang::lsp` server/types/message structs, semantic-corpus receipt verifier.

## Global Constraints

- Keep new source files under 300 lines.
- Do not add new dependencies.
- Preserve existing provider behavior; this slice only wires dispatch and response JSON.
- Use TDD: each production behavior change needs a failing focused test first.
- Keep `full VS Code extension readiness` and `compiler type-checker diagnostics in LSP` as known gaps.
- Do not claim prepare-rename or end-to-end VS Code behavior.

---

### Task 1: Raw Dispatch Tests

**Files:**
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: `dispatch_raw_message(server: &mut LanguageServer, content: &str) -> Option<String>`
- Produces: failing tests for `textDocument/codeAction` and `textDocument/rename`

- [ ] **Step 1: Add failing code action test**

Add this test to `compiler/src/lsp/server.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn raw_dispatch_code_action_returns_supplied_diagnostic_quick_fix() {
    let mut server = LanguageServer::new();
    dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn main() {\n    let x = 1\n}\n"}}}"#,
    )
    .expect("didOpen should publish diagnostics");

    let response = dispatch_raw_message(
        &mut server,
        r#"{
          "jsonrpc": "2.0",
          "id": 10,
          "method": "textDocument/codeAction",
          "params": {
            "textDocument": { "uri": "file:///workspace/main.quanta" },
            "range": {
              "start": { "line": 1, "character": 13 },
              "end": { "line": 1, "character": 13 }
            },
            "context": {
              "diagnostics": [{
                "range": {
                  "start": { "line": 1, "character": 13 },
                  "end": { "line": 1, "character": 13 }
                },
                "severity": 1,
                "source": "quantalang",
                "message": "expected ';'"
              }]
            }
          }
        }"#,
    )
    .expect("codeAction should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse response");

    assert_eq!(json["id"], 10);
    let actions = json["result"].as_array().expect("code actions array");
    assert!(actions.iter().any(|action| action["title"] == "Add missing semicolon"));
}
```

- [ ] **Step 2: Add failing rename test**

Add this test to the same module:

```rust
#[test]
fn raw_dispatch_rename_returns_workspace_edits_for_symbol_occurrences() {
    let mut server = LanguageServer::new();
    dispatch_raw_message(
        &mut server,
        r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#,
    )
    .expect("didOpen should publish diagnostics");

    let response = dispatch_raw_message(
        &mut server,
        r#"{
          "jsonrpc": "2.0",
          "id": 11,
          "method": "textDocument/rename",
          "params": {
            "textDocument": { "uri": "file:///workspace/main.quanta" },
            "position": { "line": 1, "character": 14 },
            "newName": "renamed_helper"
          }
        }"#,
    )
    .expect("rename should return a response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse response");

    assert_eq!(json["id"], 11);
    let edits = json["result"]["changes"]["file:///workspace/main.quanta"]
        .as_array()
        .expect("rename edits for document");
    assert!(edits.len() >= 2, "expected definition and call-site edits: {edits:#?}");
    assert!(edits.iter().all(|edit| edit["newText"] == "renamed_helper"));
}
```

- [ ] **Step 3: Run RED**

Run: `cargo test --manifest-path compiler/Cargo.toml --lib raw_dispatch --quiet`

Expected: the two new tests fail because `textDocument/codeAction` and `textDocument/rename` are not in raw dispatch.

### Task 2: Parser Accessors and Response Serialization

**Files:**
- Modify: `compiler/src/lsp/jsonrpc.rs`
- Create: `compiler/src/lsp/response_json.rs`
- Modify: `compiler/src/lsp/mod.rs`

**Interfaces:**
- Produces: `JsonRpcMessage::range_at(&[&str]) -> Option<Range>`
- Produces: `JsonRpcMessage::diagnostics() -> Vec<Diagnostic>`
- Produces: `JsonRpcMessage::string_vec_at(&[&str]) -> Option<Vec<String>>`
- Produces: `response_json::build_code_actions_json(actions: &[CodeAction]) -> String`
- Produces: `response_json::build_workspace_edit_json(edit: &WorkspaceEdit) -> String`

- [ ] **Step 1: Add parser accessors**

Implement accessors in `jsonrpc.rs` using `serde_json::Value`:

```rust
pub fn range_at(&self, path: &[&str]) -> Option<Range> {
    let value = self.at(path)?;
    let start = Self::position_from_value(value.get("start")?)?;
    let end = Self::position_from_value(value.get("end")?)?;
    Some(Range::new(start, end))
}
```

Also add `diagnostics()` that reads `params.context.diagnostics[]`, using severity numbers `1..=4` and `Diagnostic::error/warning/hint` style defaults.

- [ ] **Step 2: Add response serializer module**

Create `compiler/src/lsp/response_json.rs` with helpers for `Position`, `Range`, `TextEdit`, `WorkspaceEdit`, and `CodeAction`.

- [ ] **Step 3: Export module**

Add `pub mod response_json;` in `compiler/src/lsp/mod.rs`.

- [ ] **Step 4: Run compile-focused check**

Run: `cargo test --manifest-path compiler/Cargo.toml --lib code_action --quiet`

Expected: compile succeeds or fails only because raw dispatch branches are not wired yet.

### Task 3: Raw Dispatch Wiring

**Files:**
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: parser accessors and response serializers from Task 2
- Produces: raw dispatch branches for `textDocument/codeAction` and `textDocument/rename`

- [ ] **Step 1: Wire `textDocument/codeAction`**

Add a branch before unknown-method handling:

```rust
if method == "textDocument/codeAction" {
    let uri = message.text_document_uri().unwrap_or_default();
    let range = message.range_at(&["params", "range"]).unwrap_or_default();
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier { uri },
        range,
        context: CodeActionContext {
            diagnostics: message.diagnostics(),
            only: message.string_vec_at(&["params", "context", "only"]),
            trigger_kind: message.code_action_trigger_kind(),
        },
    };
    let actions = server.code_action(params);
    return Some(build_response(
        id.unwrap_or_else(|| "1".to_string()),
        response_json::build_code_actions_json(&actions),
    ));
}
```

- [ ] **Step 2: Wire `textDocument/rename`**

Add a branch:

```rust
if method == "textDocument/rename" {
    let uri = message.text_document_uri().unwrap_or_default();
    let position = message.position().unwrap_or(Position::new(0, 0));
    let new_name = message.string_at(&["params", "newName"]).unwrap_or_default();
    let params = RenameParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position,
        },
        new_name,
    };
    let result_json = server
        .rename(params)
        .as_ref()
        .map(response_json::build_workspace_edit_json)
        .unwrap_or_else(JsonBuilder::null);
    return Some(build_response(id.unwrap_or_else(|| "1".to_string()), result_json));
}
```

- [ ] **Step 3: Run GREEN**

Run: `cargo test --manifest-path compiler/Cargo.toml --lib raw_dispatch --quiet`

Expected: all raw dispatch tests pass.

### Task 4: Receipt Integration

**Files:**
- Modify: `compiler/src/lsp_dispatch/model.rs`
- Modify: `compiler/src/lsp_dispatch/fixture.rs`
- Modify: `compiler/src/lsp_dispatch/validate.rs` if comparison errors need specific wording
- Modify: `compiler/tests/cli.rs`
- Modify: `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`

**Interfaces:**
- Produces: `observed.code_actions`
- Produces: `observed.workspace_edits`

- [ ] **Step 1: Add failing receipt drift tests**

Add CLI tests that mutate `fixtures` for `textDocument/codeAction` and `textDocument/rename` observed counts and expect corpus verify rejection.

- [ ] **Step 2: Run RED**

Run: `cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture`

Expected: tests fail until the receipt model/builder includes the new methods and counts.

- [ ] **Step 3: Extend receipt model and fixture sequence**

Add `code_actions` and `workspace_edits` to `LspDispatchObserved`, add raw fixtures for `textDocument/codeAction` and `textDocument/rename`, and update observation logic.

- [ ] **Step 4: Regenerate canonical receipt**

Use a temporary ignored writer test in `compiler/src/lsp_dispatch/tests.rs`, run it, then remove the writer immediately.

- [ ] **Step 5: Run GREEN**

Run: `cargo test --manifest-path compiler/Cargo.toml --test cli lsp_dispatch -- --nocapture`

Expected: all LSP dispatch CLI tests pass.

### Task 5: Docs, Verification, Commit, Merge

**Files:**
- Modify: `compiler/src/lsp/STATUS.md`
- Modify: `docs/tutorial.md`

**Interfaces:**
- Produces: docs that state code action and rename are wired through raw dispatch, with VS Code readiness still unverified.

- [ ] **Step 1: Update docs**

Remove the statement that code actions and rename are not wired. Keep typed request coverage, semantic diagnostics, and VS Code readiness as remaining gaps.

- [ ] **Step 2: Run final verification**

Run:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib code_action --quiet
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin quantac lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git diff --check
```

- [ ] **Step 3: Commit**

Before commit:

```powershell
git check-ignore .env .env.local .env.production
git diff --cached --check
git diff --cached -U0 | Select-String -Pattern '<credential-pattern>'
```

Commit message: `feat: dispatch lsp code action and rename`
