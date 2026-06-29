# LSP Root-Backed Workspace Symbol Index Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make raw `workspace/symbol` find symbols in bounded unopened `.bld` files under a supported local workspace root while preserving opened-document overlay semantics.

**Architecture:** Add a small `WorkspaceSymbolIndex` under `compiler/src/lsp/` that scans a bounded local root and stores parsed `DocumentSymbol` trees by stable LSP URI. `LanguageServer` owns the index, rebuilds it from `initialize.rootUri`, and merges opened document symbols before indexed unopened-file symbols. The semantic-corpus LSP receipt uses a checked `semantic-corpus/lsp-workspace/` fixture mapped to `file:///workspace` so receipt digests stay machine-independent.

**Tech Stack:** Rust 2021, existing `buildlang::lsp` modules, `serde_json`, std filesystem APIs, semantic-corpus LSP dispatch receipt, Cargo test slices.

## Global Constraints

- Do not build a type-checker-backed or module-graph-backed symbol database in this slice.
- Do not resolve type, effect, trait, import, package, or module identities.
- Do not scan outside the initialized root.
- Do not scan the whole repository when `rootUri` is absent or unsupported.
- Do not index generated output, build artifacts, vendored package caches, `.git`, `target`, `node_modules`, `.worktrees`, or hidden infrastructure trees.
- Do not add live file watching or incremental filesystem invalidation yet.
- Do not claim end-to-end VS Code extension readiness.
- Use fixed caps: maximum indexed files `512`, maximum bytes read per file `256 KiB`, maximum recursion depth `16`.
- Keep new plan-controlled Rust files at or below 300 lines.

---

## File Map

- Create `compiler/src/lsp/workspace_index.rs`: root URI decoding, bounded `.bld` discovery, scan stats, indexed symbol storage.
- Modify `compiler/src/lsp/mod.rs`: export the new module.
- Modify `compiler/src/lsp/symbols.rs`: add a reusable flat-symbol helper for a known URI and pre-parsed `DocumentSymbol` tree.
- Modify `compiler/src/lsp/server.rs`: own the index, rebuild on initialize, merge opened and indexed symbols, add raw-dispatch tests.
- Modify `compiler/src/lsp_dispatch.rs`: map deterministic receipt root `file:///workspace` to `semantic-corpus/lsp-workspace/`.
- Modify `compiler/src/lsp_dispatch/fixture.rs` and `compiler/src/lsp_dispatch/tests.rs`: query an unopened file-backed symbol and assert receipt evidence.
- Add `semantic-corpus/lsp-workspace/main.bld` and `semantic-corpus/lsp-workspace/library.bld`.
- Refresh `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`.
- Modify `STATUS.md`, `compiler/src/lsp/STATUS.md`, and `docs/tutorial.md`.

---

### Task 1: Workspace Symbol Index Module

**Files:**
- Create: `compiler/src/lsp/workspace_index.rs`
- Modify: `compiler/src/lsp/mod.rs`
- Modify: `compiler/src/lsp/symbols.rs`

**Interfaces:**
- Consumes: `SymbolProvider::document_symbols(&self, doc: &Document) -> Vec<DocumentSymbol>`.
- Produces: `WorkspaceSymbolIndex`, `WorkspaceSymbolIndexStats`, `WorkspaceSymbolIndex::rebuild_from_uri`, `WorkspaceSymbolIndex::rebuild_from_path`, `WorkspaceSymbolIndex::symbols`, and `SymbolProvider::matching_symbol_information`.

- [ ] **Step 1: Write failing index tests**

Add tests in `compiler/src/lsp/workspace_index.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::document::DocumentStore;
    use crate::lsp::symbols::SymbolProvider;
    use std::sync::Arc;

    fn provider() -> SymbolProvider {
        SymbolProvider::new(Arc::new(DocumentStore::new()))
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("buildlang_lsp_index_{label}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn indexes_build_files_in_sorted_order_and_skips_excluded_dirs() {
        let root = temp_root("sorted");
        std::fs::write(root.join("b.bld"), "fn beta() -> i32 { 2 }\n").expect("write b");
        std::fs::write(root.join("a.bld"), "fn alpha() -> i32 { 1 }\n").expect("write a");
        std::fs::create_dir_all(root.join("target")).expect("create target");
        std::fs::write(root.join("target").join("hidden.bld"), "fn hidden() {}\n").expect("write hidden");

        let mut index = WorkspaceSymbolIndex::new();
        let stats = index.rebuild_from_path("file:///workspace", &root, &provider());

        assert_eq!(stats.indexed_files, 2);
        assert_eq!(index.symbols().keys().cloned().collect::<Vec<_>>(), vec![
            "file:///workspace/a.bld".to_string(),
            "file:///workspace/b.bld".to_string(),
        ]);
        assert!(!index.symbols().contains_key("file:///workspace/target/hidden.bld"));
    }

    #[test]
    fn caps_indexed_files_and_records_skips() {
        let root = temp_root("cap");
        for i in 0..(MAX_INDEXED_FILES + 1) {
            std::fs::write(root.join(format!("f{i:03}.bld")), format!("fn f{i}() {{}}\n")).expect("write file");
        }

        let mut index = WorkspaceSymbolIndex::new();
        let stats = index.rebuild_from_path("file:///workspace", &root, &provider());

        assert_eq!(stats.indexed_files, MAX_INDEXED_FILES);
        assert_eq!(stats.skipped_file_cap, 1);
    }

    #[test]
    fn rejects_unsupported_root_uri_without_panicking() {
        let mut index = WorkspaceSymbolIndex::new();
        let stats = index.rebuild_from_uri(Some("memfs:///workspace"), &provider());

        assert_eq!(stats.unsupported_root, 1);
        assert!(index.symbols().is_empty());
    }
}
```

Run: `cargo test --manifest-path compiler\Cargo.toml --lib workspace_index -- --nocapture`
Expected: FAIL because `workspace_index` does not exist.

- [ ] **Step 2: Implement module shell and exports**

Create `compiler/src/lsp/workspace_index.rs` with constants, stats, and storage:

```rust
pub(crate) const MAX_INDEXED_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_RECURSION_DEPTH: usize = 16;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceSymbolIndexStats {
    pub(crate) indexed_files: usize,
    pub(crate) skipped_file_cap: usize,
    pub(crate) skipped_large_files: usize,
    pub(crate) skipped_non_utf8: usize,
    pub(crate) skipped_read_errors: usize,
    pub(crate) unsupported_root: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WorkspaceSymbolIndex {
    symbols_by_uri: std::collections::BTreeMap<String, Vec<super::types::DocumentSymbol>>,
    stats: WorkspaceSymbolIndexStats,
}
```

In `compiler/src/lsp/mod.rs`, add `pub mod workspace_index;`. In `symbols.rs`, add:

```rust
pub fn matching_symbol_information(
    &self,
    symbols: &[DocumentSymbol],
    uri: &str,
    query_lower: &str,
) -> Vec<SymbolInformation> {
    let mut result = Vec::new();
    self.collect_matching_symbols(symbols, uri, query_lower, &mut result);
    result
}
```

- [ ] **Step 3: Implement bounded scan**

Implement `WorkspaceSymbolIndex::new`, `stats`, `symbols`, `rebuild_from_uri`, and `rebuild_from_path`. The path-backed rebuild must sort directory entries by path, skip excluded directory names, stop after `MAX_INDEXED_FILES`, read only UTF-8 files at or below `MAX_FILE_BYTES`, and create `Document::new(uri, "build".to_string(), 0, content)` before calling `symbols.document_symbols(&doc)`.

Run: `cargo test --manifest-path compiler\Cargo.toml --lib workspace_index -- --nocapture`
Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add compiler\src\lsp\workspace_index.rs compiler\src\lsp\mod.rs compiler\src\lsp\symbols.rs
git commit -m "feat: add lsp workspace symbol index"
```

---

### Task 2: Server Integration And Overlay Semantics

**Files:**
- Modify: `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: `WorkspaceSymbolIndex::rebuild_from_uri`, `WorkspaceSymbolIndex::rebuild_from_path`, `WorkspaceSymbolIndex::symbols`, `SymbolProvider::matching_symbol_information`.
- Produces: `LanguageServer::workspace_symbol(&self, query: &str) -> Vec<SymbolInformation>` that merges opened documents first and indexed unopened files second.

- [ ] **Step 1: Write failing raw dispatch tests**

Add tests in `compiler/src/lsp/server.rs`:

```rust
#[test]
fn raw_dispatch_workspace_symbol_returns_unopened_root_file_symbol() {
    let root = temp_workspace_root("unopened");
    std::fs::write(root.join("library.bld"), "fn library_helper() -> i32 { 7 }\n").expect("write library");
    let root_uri = path_file_uri(&root);
    let mut server = LanguageServer::new();
    dispatch_raw_message(&mut server, &format!(r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{root_uri}"}}}}"#)).expect("initialize response");

    let response = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","id":40,"method":"workspace/symbol","params":{"query":"library_helper"}}"#).expect("workspace response");
    let json: serde_json::Value = serde_json::from_str(&response).expect("parse response");
    assert!(json["result"].as_array().expect("result array").iter().any(|symbol| symbol["name"] == "library_helper"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn raw_dispatch_open_document_overrides_indexed_file_symbol() {
    let root = temp_workspace_root("override");
    let file = root.join("main.bld");
    std::fs::write(&file, "fn disk_only() -> i32 { 1 }\n").expect("write disk file");
    let root_uri = path_file_uri(&root);
    let file_uri = path_file_uri(&file);
    let mut server = LanguageServer::new();
    dispatch_raw_message(&mut server, &format!(r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{root_uri}"}}}}"#)).expect("initialize response");
    dispatch_raw_message(&mut server, &format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{file_uri}","languageId":"build","version":1,"text":"fn editor_only() -> i32 {{ 2 }}\n"}}}}}}"#)).expect("didOpen response");

    let disk = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","id":41,"method":"workspace/symbol","params":{"query":"disk_only"}}"#).expect("disk response");
    let editor = dispatch_raw_message(&mut server, r#"{"jsonrpc":"2.0","id":42,"method":"workspace/symbol","params":{"query":"editor_only"}}"#).expect("editor response");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&disk).unwrap()["result"].as_array().unwrap().len(), 0);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&editor).unwrap()["result"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(root);
}
```

Run: `cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch -- --nocapture`
Expected: FAIL because `LanguageServer` does not own or query the new index.

- [ ] **Step 2: Wire the index into LanguageServer**

Add `workspace_index: WorkspaceSymbolIndex` to `LanguageServer`, initialize it in `new`, rebuild it in `initialize` from a cloned `root_uri`, and rewrite `workspace_symbol` so it:

1. lowercases the query once,
2. sorts opened document URIs,
3. collects matches from opened documents,
4. skips indexed URIs that are already open,
5. collects matches from `workspace_index.symbols()` in `BTreeMap` order.

Also add `pub(crate) fn rebuild_workspace_symbol_index_for_root(&mut self, root_uri: &str, root_path: &std::path::Path) -> WorkspaceSymbolIndexStats` for deterministic receipt fixtures.

Run: `cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet`
Expected: PASS.

- [ ] **Step 3: Commit**

```powershell
git add compiler\src\lsp\server.rs
git commit -m "feat: index root-backed lsp workspace symbols"
```

---

### Task 3: Receipt Fixture For Unopened Root File

**Files:**
- Add: `semantic-corpus/lsp-workspace/main.bld`
- Add: `semantic-corpus/lsp-workspace/library.bld`
- Modify: `compiler/src/lsp_dispatch.rs`
- Modify: `compiler/src/lsp_dispatch/fixture.rs`
- Modify: `compiler/src/lsp_dispatch/tests.rs`
- Modify: `compiler/tests/cli.rs`
- Modify: `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`

**Interfaces:**
- Consumes: `LanguageServer::rebuild_workspace_symbol_index_for_root`.
- Produces: checked receipt evidence that `workspace/symbol` finds an unopened file-backed symbol.

- [ ] **Step 1: Add failing receipt assertions**

Add checked fixture files:

```rust
// semantic-corpus/lsp-workspace/main.bld
fn opened_helper() -> i32 { 1 }

// semantic-corpus/lsp-workspace/library.bld
fn library_helper() -> i32 { 2 }
```

Change `workspace-symbol` query in `fixture.rs` from `"help"` to `"library_helper"`. In `tests.rs`, assert:

```rust
assert_eq!(fixture.method, "workspace/symbol");
assert_eq!(fixture.observed.workspace_symbols, 1);
```

Run: `cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch -- --nocapture`
Expected: FAIL until `lsp_dispatch.rs` maps the fixture path into the server index.

- [ ] **Step 2: Map deterministic fixture root**

In `compiler/src/lsp_dispatch.rs`, before dispatching fixtures, call:

```rust
server.rebuild_workspace_symbol_index_for_root("file:///workspace", &root.join("lsp-workspace"));
```

This keeps response URIs stable as `file:///workspace/library.bld`.

- [ ] **Step 3: Refresh receipt**

Temporarily add the ignored writer test used in prior LSP receipt tasks, run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin buildc write_semantic_corpus_lsp_dispatch_receipt -- --ignored --nocapture
```

Remove the writer immediately after it rewrites `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`.

- [ ] **Step 4: Verify and commit**

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git add compiler\src\lsp_dispatch.rs compiler\src\lsp_dispatch\fixture.rs compiler\src\lsp_dispatch\tests.rs compiler\tests\cli.rs semantic-corpus\lsp-workspace semantic-corpus\receipts\lsp-dispatch-2026-06-18.json
git commit -m "test: prove root-backed lsp workspace symbols"
```

---

### Task 4: Documentation And Final Verification

**Files:**
- Modify: `compiler/src/lsp/STATUS.md`
- Modify: `STATUS.md`
- Modify: `docs/tutorial.md`

**Interfaces:**
- Consumes: verified root-backed workspace-symbol behavior and receipt evidence.
- Produces: accurate public status text.

- [ ] **Step 1: Update docs**

State that `workspace/symbol` is root-backed for bounded local `.bld` files plus opened documents. Keep compiler-resolved global symbol identity and end-to-end VS Code behavior as open gaps.

- [ ] **Step 2: Run final targeted verification**

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib workspace_index --quiet
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

Expected: all commands exit `0`.

- [ ] **Step 3: Commit docs and hygiene**

```powershell
git add STATUS.md compiler\src\lsp\STATUS.md docs\tutorial.md
git commit -m "docs: record root-backed lsp workspace symbols"
git check-ignore .env .env.local .env.production
git diff --check origin/main..HEAD
git diff --name-only origin/main..HEAD
```

Expected: env files are ignored, diff check prints no errors, changed file list contains no `.env` files.
