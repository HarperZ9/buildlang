# Symbol Graph Receipts v0 Implementation Plan
> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a checked symbol graph receipt that ties semantic-corpus source symbols to MIR symbols and type-checker effect summaries.

**Architecture:** Add a focused `symbol_graph` receipt module beside the MIR representation and memory layout receipt modules. Extend `LoweredMirProgram` so the existing parser/type-check/MIR lowering path also returns the parsed AST and cloned effect summaries for receipt construction.

**Tech Stack:** Rust, serde JSON DTOs, BTreeMap/BTreeSet deterministic ordering, SHA-256, existing `buildc corpus verify` CLI tests.

## Global Constraints
- Receipt schema must be exactly `buildlang-symbol-graph-receipt/v0`.
- Checked receipt path must be exactly `semantic-corpus/receipts/symbol-graph-2026-06-18.json`.
- Substrate reference must be exactly `symbol_surface.symbol_receipt`.
- Successful `buildc corpus verify` output must include `symbol graph receipt: ok`.
- Program and receipt paths must reject empty strings, absolute paths, rooted paths, Windows drive paths, UNC/rooted backslash paths, and `..` components.
- Arrays emitted by the receipt builder must be sorted and deduplicated.
- v0 must not claim full call graph, package public API graph, external package resolution, LSP readiness, cross-file editor navigation, a public symbol dump command, receipt writing, backend productionization, self-hosting, or cryptographic signing.

---

## File Structure
- `compiler/src/mir_representation.rs`: add `LoweredMirProgram.ast`, `LoweredMirProgram.function_effect_summaries`, and `collect_mir_symbols`.
- `compiler/src/symbol_graph.rs`: new DTO, builder, validator, digest, path, source symbol, effect symbol, edge, and unit-test module.
- `compiler/src/main.rs`: register symbol graph module, verify receipt in `corpus verify`, add and validate `SubstrateSymbolSurface`.
- `compiler/tests/cli.rs`: add fixture helper and symbol graph receipt regression tests.
- `semantic-corpus/receipts/symbol-graph-2026-06-18.json`: checked receipt for the current eight-program corpus.
- `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`: add `symbol_surface`.
- `README.md`, `STATUS.md`: document the evidence boundary.

## Interfaces
- `LoweredMirProgram` fields: `source_digest: MirRepresentationDigest`, `input_graph_digest: MirRepresentationDigest`, `ast: buildlang::ast::Module`, `function_effect_summaries: Vec<buildlang::types::FunctionEffectSummary>`, `module: buildlang::mir::MirModule`.
- `pub(crate) fn collect_mir_symbols(module: &MirModule) -> MirRepresentationSymbols`.
- `pub(crate) const SYMBOL_GRAPH_RECEIPT: &str = "symbol-graph-2026-06-18.json"`.
- `pub(crate) fn build_symbol_graph_receipt(corpus_root: &Path, manifest: &SemanticCorpusManifest) -> Result<SymbolGraphReceipt, String>`.
- `pub(crate) fn validate_symbol_graph_receipt(corpus_root: &Path, receipt: &SymbolGraphReceipt, manifest: &SemanticCorpusManifest) -> Result<(), String>`.
- `pub(crate) fn verify_symbol_graph_receipt(corpus_root: &Path, receipt: &SymbolGraphReceipt, manifest: &SemanticCorpusManifest) -> Result<(), i32>`.

### Task 1: Red CLI Coverage
**Files:** Modify `compiler/tests/cli.rs`.
**Interfaces:** Consumes `temp_semantic_corpus`, `write_substrate_receipt_copy`, `buildc`; produces `write_symbol_graph_receipt_copy`.
- [ ] **Step 1: Add the fixture helper** after `write_memory_layout_receipt_copy`.
```rust
fn write_symbol_graph_receipt_copy(corpus_root: &Path, transform: impl FnOnce(serde_json::Value) -> serde_json::Value) {
    let receipt_path = corpus_root.join("receipts").join("symbol-graph-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read symbol graph receipt"))
            .expect("parse symbol graph receipt");
    let rendered = serde_json::to_string_pretty(&transform(receipt))
        .expect("render modified symbol graph receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified symbol graph receipt");
}
```
- [ ] **Step 2: Add the rejection helper** beside the symbol graph tests.
```rust
fn assert_corpus_verify_rejects(corpus_root: &Path, expected_stderr: &str) {
    let output = buildc().arg("corpus").arg("verify").arg("--root").arg(corpus_root)
        .output().expect("run buildc corpus verify against symbol graph fixture");
    let _ = fs::remove_dir_all(corpus_root);
    assert!(!output.status.success(), "fixture should fail");
    assert!(String::from_utf8_lossy(&output.stderr).contains(expected_stderr),
        "stderr should contain {expected_stderr:?}:\n{}", String::from_utf8_lossy(&output.stderr));
}
```
- [ ] **Step 3: Add these exact CLI tests.**

| Test | Mutation | Expected |
| --- | --- | --- |
| `corpus_verify_checks_symbol_graph_receipt` | none; use repository corpus | stdout contains `symbol graph receipt: ok` |
| `corpus_verify_rejects_symbol_graph_schema_drift` | set `schema` to `buildlang-symbol-graph-receipt/v9` | `symbol graph receipt has unsupported schema` |
| `corpus_verify_rejects_symbol_graph_program_count_drift` | set `source_set.program_count` to `7` | `symbol graph source_set.program_count mismatch` |
| `corpus_verify_rejects_symbol_graph_path_escape` | set `programs[0].path` to `../outside.bld` | `symbol graph program.path must stay within corpus root` |
| `corpus_verify_rejects_symbol_graph_source_digest_drift` | set `programs[0].source_digest.hex` to `"0".repeat(64)` | `symbol graph program scalar_branch source_digest mismatch` |
| `corpus_verify_rejects_symbol_graph_source_symbol_drift` | set `programs[0].source_symbols` to `[]` | `symbol graph program scalar_branch source_symbols drift` |
| `corpus_verify_rejects_symbol_graph_mir_symbol_drift` | set `programs[0].mir_symbols.functions` to `["forged"]` | `symbol graph program scalar_branch mir_symbols drift` |
| `corpus_verify_rejects_symbol_graph_edge_drift` | set `programs[0].edges` to `[]` | `symbol graph program scalar_branch edges drift` |
| `corpus_verify_rejects_symbol_graph_lsp_overclaim` | set `symbol_model.semantic_anchor` to `LSP request dispatch verified` | `symbol graph symbol_model drift` |
| `corpus_verify_rejects_substrate_symbol_receipt_path_escape` | set substrate `symbol_surface.symbol_receipt` to `../symbol.json` | `substrate symbol_surface.symbol_receipt must stay within corpus root` |

- [ ] **Step 4: Run the red tests.** Run `cargo test --manifest-path compiler/Cargo.toml --test cli symbol_graph -- --nocapture`. Expected: the slice fails because receipt JSON, `symbol_surface`, and verifier wiring are absent.
- [ ] **Step 5: Commit.** Run `git add compiler/tests/cli.rs` then `git commit -m "test: require symbol graph receipts"`.

### Task 2: Extend MIR Lowering Evidence
**Files:** Modify `compiler/src/mir_representation.rs`.
**Interfaces:** Produces AST/effect evidence for `symbol_graph.rs` while preserving MIR and memory receipt behavior.
- [ ] **Step 1: Add imports.** Use `buildlang::ast::Module` and `buildlang::types::FunctionEffectSummary`.
- [ ] **Step 2: Extend `LoweredMirProgram`.** Add `ast: Module` and `function_effect_summaries: Vec<FunctionEffectSummary>` before `module`.
- [ ] **Step 3: Extract MIR symbols.** Add this function and call it from `summarize_mir_program`.
```rust
pub(crate) fn collect_mir_symbols(module: &MirModule) -> MirRepresentationSymbols {
    MirRepresentationSymbols {
        functions: sorted(module.functions.iter().map(|function| function.name.to_string()).collect()),
        types: sorted(module.types.iter().map(|ty| ty.name.to_string()).collect()),
        globals: sorted(module.globals.iter().map(|global| global.name.to_string()).collect()),
        externals: sorted(module.externals.iter().map(|external| external.name.to_string()).collect()),
    }
}
```
- [ ] **Step 4: Return AST and effect summaries.** After type checking succeeds, add `let function_effect_summaries = checker.function_effect_summaries().to_vec();`. Return `ast` and `function_effect_summaries` in `LoweredMirProgram`.
- [ ] **Step 5: Verify and commit.**
```bash
cargo test --manifest-path compiler/Cargo.toml --bin buildc mir_representation --quiet
cargo test --manifest-path compiler/Cargo.toml --bin buildc memory_layout --quiet
git add compiler/src/mir_representation.rs
git commit -m "feat: expose lowering evidence for symbol receipts"
```
Expected: both test commands pass before the commit.

### Task 3: Implement `symbol_graph.rs`
**Files:** Create `compiler/src/symbol_graph.rs`; modify `compiler/src/main.rs`.
**Interfaces:** Consumes `lower_program_to_mir`, `digest_mir_module`, `collect_mir_symbols`, `MirRepresentationDigest`, `MirRepresentationSymbols`, `SemanticCorpusManifest`, `SemanticCorpusProgram`; produces `SymbolGraphReceipt`, `build_symbol_graph_receipt`, `validate_symbol_graph_receipt`, `verify_symbol_graph_receipt`.
- [ ] **Step 1: Register module imports.** Add `mod symbol_graph;` and `use symbol_graph::{verify_symbol_graph_receipt, SymbolGraphReceipt, SYMBOL_GRAPH_RECEIPT};`.
- [ ] **Step 2: Define DTOs.** Define `SymbolGraphReceipt`, `SymbolGraphSourceSet`, `SymbolGraphModel`, `SymbolGraphProgram`, `SymbolSourceSymbol`, `SymbolSourceSpan`, `SymbolSourceSignature`, `SymbolGraphEffectSymbol`, `SymbolGraphEdge`, and `SymbolGraphSummary`; every DTO derives `Clone`, `Debug`, `PartialEq`, `Eq`, `serde::Deserialize`, and `serde::Serialize`.
- [ ] **Step 3: Implement helpers.** Implement `sorted`, `dedup_sorted`, `digest_hex`, `symbol_digest`, `visibility_name`, `path_last_name`, `is_lexically_invalid_relative_path`, and `validate_corpus_relative_path`; use the memory layout lexical path checks.
- [ ] **Step 4: Collect source symbols.** Implement `collect_source_symbols(&Module)`. Cover top-level functions, structs, enums, traits, type aliases, consts, statics, modules, macros, macro rules, effects, struct fields, tuple fields, enum variants, trait functions/types/consts, impl functions/types/consts, extern functions/statics/types, and effect operations. Use IDs like `source:function:main`, `source:struct:Point.field:x`, `source:struct:Tuple.field:0`, `source:enum:Result.variant:Ok`, `source:trait:Display.method:fmt`, `source:impl.method:fmt`, `source:extern:function:puts`, and `source:effect:Console.operation:write`.
- [ ] **Step 5: Collect effect symbols and edges.** Implement `collect_effect_symbols(&[FunctionEffectSummary])` with sorted declared effects, observed capability effect names, and propagated `effect:source` strings. Implement `collect_edges` with only `source_to_mir_function`, `source_to_mir_type`, `source_to_mir_external`, and `source_to_effect_summary`.
- [ ] **Step 6: Build and validate receipts.** Build each program from `lower_program_to_mir`, `digest_mir_module`, `collect_mir_symbols`, source symbols, effect symbols, edges, and known gaps. Validate scalar fields before rebuilding; compare with diagnostics including `symbol graph symbol_model drift`, `symbol graph program <id> source_digest mismatch`, `symbol graph program <id> source_symbols drift`, `symbol graph program <id> mir_symbols drift`, `symbol graph program <id> edges drift`, and `symbol graph summary drift`.
- [ ] **Step 7: Add unit tests.** Test sorted/deduped source symbols, exact-match edge creation, effect symbol sorting, lexical path rejection for `../x.bld`, `C:\x.bld`, `C:x.bld`, and `\x.bld`, and symbol model drift rejection.
- [ ] **Step 8: Verify and commit.**
```bash
cargo test --manifest-path compiler/Cargo.toml --bin buildc symbol_graph --quiet
git add compiler/src/main.rs compiler/src/symbol_graph.rs compiler/src/mir_representation.rs
git commit -m "feat: build symbol graph receipts"
```
Expected: the unit test slice passes before the commit.

### Task 4: Wire Corpus And Substrate Verification
**Files:** Modify `compiler/src/main.rs` and `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`.
**Interfaces:** Produces `symbol graph receipt: ok` in successful corpus verification and checked substrate path containment.
- [ ] **Step 1: Verify the receipt in `verify_semantic_corpus`.** Read `receipts_dir.join(SYMBOL_GRAPH_RECEIPT)` as `SymbolGraphReceipt`, call `verify_symbol_graph_receipt`, and print `symbol graph receipt: ok` after the memory layout status line.
- [ ] **Step 2: Add substrate symbol DTO.** Add `symbol_surface: SubstrateSymbolSurface` to `SubstrateReceipt` and define `SubstrateSymbolSurface { source, representation, effect_anchor, symbol_receipt, known_gaps }`.
- [ ] **Step 3: Validate substrate symbol surface.** Require source `AST`, representation `MIR`, effect anchor `buildlang-check-receipt/v1`, nonempty known gaps, and canonical equality to `receipts/symbol-graph-2026-06-18.json`.
- [ ] **Step 4: Update substrate JSON.**
```json
"symbol_surface": {
  "source": "AST",
  "representation": "MIR",
  "effect_anchor": "buildlang-check-receipt/v1",
  "symbol_receipt": "receipts/symbol-graph-2026-06-18.json",
  "known_gaps": ["cross-package public API index", "full LSP request dispatch", "full package graph resolution"]
}
```
- [ ] **Step 5: Verify and commit.**
```bash
cargo test --manifest-path compiler/Cargo.toml --test cli symbol_graph -- --nocapture
git add compiler/src/main.rs semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json
git commit -m "feat: verify symbol graph receipt"
```
Expected: the CLI slice fails only when the checked symbol graph receipt JSON is still absent; after adding a temporary receipt during local development it reaches the drift assertions.

### Task 5: Add Checked Receipt And Docs
**Files:** Create `semantic-corpus/receipts/symbol-graph-2026-06-18.json`; modify `README.md` and `STATUS.md`.
**Interfaces:** Consumes `build_symbol_graph_receipt`; produces checked JSON and evidence-boundary docs.
- [ ] **Step 1: Generate the checked receipt.** Create pretty JSON with a trailing newline. It must contain eight programs and must equal `build_symbol_graph_receipt(repo_root/semantic-corpus, manifest)`.
- [ ] **Step 2: Update docs.** Add one sentence to `README.md` and one sentence to `STATUS.md`: symbol graph receipts are checked source/MIR/effect symbol evidence and do not prove call graph, LSP readiness, or package API completion.
- [ ] **Step 3: Run acceptance verification.**
```bash
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo test --manifest-path compiler/Cargo.toml --bin buildc symbol_graph --quiet
cargo test --manifest-path compiler/Cargo.toml --bin buildc mir_representation --quiet
cargo test --manifest-path compiler/Cargo.toml --bin buildc memory_layout --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli symbol_graph -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus
git diff --check
git check-ignore .env .env.local
git diff --cached -- . | rg -n "(api[_-]?key|secret|token|password|BEGIN [A-Z ]+PRIVATE KEY)" -i
```
Expected: test and corpus commands pass; `corpus verify` prints `symbol graph receipt: ok`; `.env` paths are ignored; secret scan has no matches.
- [ ] **Step 4: Commit.** Run `git add semantic-corpus/receipts/symbol-graph-2026-06-18.json README.md STATUS.md` then `git commit -m "docs: record symbol graph receipt"`.

## Self-Review Checklist
- Spec coverage: schema, canonical path, substrate field, corpus output, source/MIR/effect bridge, path containment, sorted arrays, drift diagnostics, overclaim rejection, docs, and narrow verification are covered.
- Type consistency: `LoweredMirProgram.ast`, `function_effect_summaries`, `collect_mir_symbols`, `SYMBOL_GRAPH_RECEIPT`, `SymbolGraphReceipt`, and `SubstrateSymbolSurface` are introduced before use.
- Test coverage: valid status, schema drift, program count drift, path escape, source digest drift, source symbol drift, MIR symbol drift, edge drift, overclaim drift, substrate path escape, sorted collection, and exact-match edges are covered.
