# LSP Semantic Tokens v0 Implementation Plan
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add receipt-verified full-document LSP semantic tokens for open BuildLang documents.

**Architecture:** Create a focused `compiler/src/lsp/semantic_tokens.rs` provider that scans an open `Document`, classifies v0 token kinds, and emits LSP relative token data. Keep `server.rs` responsible for routing and response JSON; extend the existing LSP dispatch receipt to prove the new method.

**Tech Stack:** Rust 2021, existing `buildlang::lsp` modules, `serde_json`, semantic-corpus LSP receipt verifier, Cargo test slices.

## Global Constraints

- Do not claim end-to-end VS Code readiness or full compiler semantic indexing.
- Do not add range tokens, delta tokens, token refresh, or progress.
- Do not require a successful full parse before returning tokens.
- Missing or malformed `params.textDocument.uri` returns `-32602 Invalid params`.
- Unknown document URI returns successful `null`.
- Keep new/touched plan-controlled files at or below 300 lines.
- Use targeted LSP and corpus verification slices.

---

## File Map

- Create `compiler/src/lsp/semantic_tokens.rs`; modify `compiler/src/lsp/mod.rs`.
- Modify `compiler/src/lsp/{types.rs,response_json.rs,server.rs}` for capability, JSON, route, and tests.
- Modify `compiler/src/lsp_dispatch/{model.rs,observe.rs,fixture.rs,tests.rs}`, `compiler/tests/cli.rs`, and `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json` for receipt evidence.
- Modify `STATUS.md`, `compiler/src/lsp/STATUS.md`, and `docs/tutorial.md` for honest status updates.

---

### Task 1: Semantic Token Provider

**Files:**
- Create: `compiler/src/lsp/semantic_tokens.rs`
- Modify: `compiler/src/lsp/mod.rs`

**Interfaces:**
- Consumes: `crate::lsp::document::Document`.
- Produces: `SemanticTokensProvider::new() -> Self`, `SemanticTokensProvider::legend() -> SemanticTokenLegendSpec`, `SemanticTokensProvider::full(&self, doc: &Document) -> SemanticTokens`, and `SemanticTokens { data: Vec<u32> }`.

- [ ] **Step 1: Write failing provider tests**

Create `compiler/src/lsp/semantic_tokens.rs` with tests shaped like:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::document::Document;

    fn doc(source: &str) -> Document {
        Document::new("file:///main.bld".into(), "build".into(), 1, source.into())
    }

    #[test]
    fn semantic_tokens_encode_core_buildlang_surface() {
        let tokens = SemanticTokensProvider::new().full(&doc(
            "// comment\nfn helper() -> i32 { 42 }\nfn main() { helper(\"x\"); }\n",
        ));
        assert_eq!(tokens.data.len() % 5, 0);
        let decoded = decode_absolute(&tokens.data);
        assert!(decoded.iter().any(|t| t.line == 0 && t.token_type == 7));
        assert!(decoded.iter().any(|t| t.line == 1 && t.start == 0 && t.token_type == 6));
        assert!(decoded.iter().any(|t| t.line == 1 && t.start == 3 && t.token_type == 2 && t.modifiers == 3));
        assert!(decoded.iter().any(|t| t.line == 1 && t.token_type == 9));
        assert!(decoded.iter().any(|t| t.line == 2 && t.token_type == 8));
    }

    #[test]
    fn semantic_tokens_return_best_effort_for_malformed_source() {
        let tokens = SemanticTokensProvider::new().full(&doc("fn broken(\"unterminated\nlet x = 1\n"));
        assert!(!tokens.data.is_empty());
        assert_eq!(tokens.data.len() % 5, 0);
    }

    struct AbsoluteToken { line: u32, start: u32, token_type: u32, modifiers: u32 }
    fn decode_absolute(data: &[u32]) -> Vec<AbsoluteToken> {
        let (mut line, mut start) = (0, 0);
        data.chunks_exact(5).map(|chunk| {
            line += chunk[0];
            start = if chunk[0] == 0 { start + chunk[1] } else { chunk[1] };
            AbsoluteToken { line, start, token_type: chunk[3], modifiers: chunk[4] }
        }).collect()
    }
}
```

- [ ] **Step 2: Run RED provider slice**

Run: `cargo test --manifest-path compiler\Cargo.toml --lib semantic_tokens -- --nocapture`
Expected: FAIL because `SemanticTokensProvider` and `SemanticTokens` do not exist.

- [ ] **Step 3: Implement provider**

Add public types:

```rust
use super::document::Document;

pub const TOKEN_TYPES: &[&str] = &["namespace","type","function","variable","parameter","property","keyword","comment","string","number","operator","macro"];
pub const TOKEN_MODIFIERS: &[&str] = &["declaration","definition","readonly","static","deprecated"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTokens { pub data: Vec<u32> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTokenLegendSpec {
    pub token_types: Vec<&'static str>,
    pub token_modifiers: Vec<&'static str>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SemanticTokensProvider;
```

Implement `new`, `legend`, and `full`. The scanner walks each line and emits comments, strings, numbers, identifiers, and exact spec operators. Use token indexes from `TOKEN_TYPES`; modifier bit `1 << 0` is declaration and `1 << 1` is definition. Function names after `fn` and call identifiers before `(` are `function`; identifiers after `struct`, `enum`, `trait`, `impl`, and `type` are `type`; identifiers after `let` and `const` are declarations.

- [ ] **Step 4: Export module and run GREEN**

Add `pub mod semantic_tokens;` to `compiler/src/lsp/mod.rs`.

Run: `cargo test --manifest-path compiler\Cargo.toml --lib semantic_tokens -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit provider**

```powershell
git add compiler\src\lsp\semantic_tokens.rs compiler\src\lsp\mod.rs
git commit -m "feat: add lsp semantic token provider"
```

---

### Task 2: Server Capability and Raw Dispatch

**Files:**
- Modify: `compiler/src/lsp/types.rs`, `compiler/src/lsp/response_json.rs`, `compiler/src/lsp/server.rs`

**Interfaces:**
- Consumes: `SemanticTokensProvider`, `SemanticTokens`, `SemanticTokenLegendSpec`, and `raw_params::decode_document_uri`.
- Produces: `LanguageServer::semantic_tokens(&self, uri: &DocumentUri) -> Option<SemanticTokens>`, `response_json::build_semantic_tokens_json(&SemanticTokens) -> String`, and `response_json::build_semantic_tokens_options_json(&SemanticTokenLegendSpec) -> String`.

- [ ] **Step 1: Write failing raw dispatch tests**

In `compiler/src/lsp/server.rs`, add tests for initialize capability, non-empty token data for an opened document, missing URI invalid params, and unknown URI null. Use method `"textDocument/semanticTokens/full"` and source `"// comment\nfn helper() -> i32 { 42 }\nfn main() { helper(\"x\"); }\n"`.

- [ ] **Step 2: Run RED raw dispatch slice**

Run: `cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch -- --nocapture`
Expected: FAIL because initialize omits `semanticTokensProvider` and the method is not routed.

- [ ] **Step 3: Enable capability model**

In `types.rs`, import `SemanticTokensProvider` and set:

```rust
semantic_tokens_provider: Some(SemanticTokensOptions {
    legend: SemanticTokensLegend {
        token_types: SemanticTokensProvider::legend().token_types.into_iter().map(str::to_string).collect(),
        token_modifiers: SemanticTokensProvider::legend().token_modifiers.into_iter().map(str::to_string).collect(),
    },
    range: false,
    full: true,
}),
```

- [ ] **Step 4: Add response serializers**

In `response_json.rs`, import semantic-token types and add:

```rust
pub fn build_semantic_tokens_json(tokens: &SemanticTokens) -> String {
    serde_json::to_string(&json!({ "data": tokens.data })).expect("serialize semantic tokens")
}

pub fn build_semantic_tokens_options_json(legend: &SemanticTokenLegendSpec) -> String {
    serde_json::to_string(&json!({
        "legend": { "tokenTypes": legend.token_types, "tokenModifiers": legend.token_modifiers },
        "range": false,
        "full": true
    })).expect("serialize semantic token options")
}
```

- [ ] **Step 5: Add server route**

In `server.rs`, import `SemanticTokensProvider`, add `semantic_tokens: SemanticTokensProvider` to `LanguageServer`, initialize it with `SemanticTokensProvider::new()`, add `semantic_tokens(&self, uri: &DocumentUri)`, include `semanticTokensProvider` in `build_initialize_result`, and add a raw branch that decodes document URI and returns serialized tokens or `null`.

- [ ] **Step 6: Run GREEN and commit**

Run: `cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch -- --nocapture`
Expected: PASS.

```powershell
git add compiler\src\lsp\types.rs compiler\src\lsp\response_json.rs compiler\src\lsp\server.rs
git commit -m "feat: dispatch lsp semantic tokens"
```

---

### Task 3: Receipt Coverage

**Files:**
- Modify: `compiler/src/lsp_dispatch/model.rs`, `compiler/src/lsp_dispatch/observe.rs`, `compiler/src/lsp_dispatch/fixture.rs`, `compiler/src/lsp_dispatch/tests.rs`, `compiler/tests/cli.rs`, `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`

**Interfaces:**
- Produces: `LspDispatchObserved.semantic_tokens: usize`.

- [ ] **Step 1: Write failing receipt tests**

Add an `lsp_dispatch` unit test that finds fixture id `"semantic-tokens"` and asserts `observed.semantic_tokens > 0`. Add a CLI drift test that sets that count to `0` and expects `lsp dispatch fixture semantic-tokens observed drift`.

- [ ] **Step 2: Run RED receipt slice**

Run: `cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch -- --nocapture`
Expected: FAIL because `semantic_tokens` is not modeled or observed.

- [ ] **Step 3: Model, observe, and fixture semantic tokens**

Add field:

```rust
#[serde(default, skip_serializing_if = "is_zero")]
pub(crate) semantic_tokens: usize,
```

Initialize it to `0` and add observe branch:

```rust
"textDocument/semanticTokens/full" => {
    observed.semantic_tokens = value.pointer("/result/data")
        .and_then(Value::as_array)
        .map_or(0, |data| data.len() / 5);
}
```

Insert fixture after `did-open`:

```rust
text_document_fixture(12, "semantic-tokens", "textDocument/semanticTokens/full", &document),
```

- [ ] **Step 4: Refresh receipt**

Temporarily add an ignored writer test in `compiler/src/lsp_dispatch/tests.rs` that writes `build_lsp_dispatch_receipt` as pretty JSON to `semantic-corpus/receipts/lsp-dispatch-2026-06-18.json`. Run it, then remove the writer before committing.

Run: `cargo test --manifest-path compiler\Cargo.toml --bin buildc write_semantic_corpus_lsp_dispatch_receipt -- --ignored --nocapture`
Expected: PASS and the receipt JSON changes.

- [ ] **Step 5: Run GREEN receipt slices and commit**

```powershell
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
git add compiler\src\lsp_dispatch compiler\tests\cli.rs semantic-corpus\receipts\lsp-dispatch-2026-06-18.json
git commit -m "test: verify lsp semantic token receipt"
```

Expected: all PASS; `corpus verify` prints `lsp dispatch receipt: ok`.

---

### Task 4: Documentation and Final Verification

**Files:**
- Modify: `compiler/src/lsp/STATUS.md`, `STATUS.md`, `docs/tutorial.md`

**Interfaces:**
- Consumes: verified semantic-token dispatch and receipt evidence.
- Produces: accurate public status text.

- [ ] **Step 1: Update docs**

Document that LSP semantic tokens v0 are implemented and receipt-verified. Keep `full VS Code extension readiness` as the remaining gap and do not claim full compiler-backed semantic indexing.

- [ ] **Step 2: Run final targeted verification**

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
cargo test --manifest-path compiler\Cargo.toml --lib semantic_tokens --quiet
cargo test --manifest-path compiler\Cargo.toml --lib raw_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --bin buildc lsp_dispatch --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli lsp_dispatch -- --nocapture
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

Expected: all PASS.

- [ ] **Step 3: Commit docs and run hygiene**

```powershell
git add STATUS.md compiler\src\lsp\STATUS.md docs\tutorial.md
git commit -m "docs: record lsp semantic token support"
git check-ignore .env .env.local .env.production
git diff --check origin/main..HEAD
git diff --name-only origin/main..HEAD
```

Expected: `.env`, `.env.local`, and `.env.production` are ignored; diff check has no output; changed file list has no secrets or `.env` files.
