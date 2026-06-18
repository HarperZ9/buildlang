use std::path::{Path, PathBuf};

use super::*;

fn repo_semantic_corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have repository parent")
        .join("semantic-corpus")
}

fn read_manifest(root: &Path) -> Result<SemanticCorpusManifest, String> {
    serde_json::from_slice(
        &std::fs::read(root.join("manifest.json")).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

fn built_receipt() -> (PathBuf, SemanticCorpusManifest, LspDispatchReceipt) {
    let root = repo_semantic_corpus_root();
    let manifest = read_manifest(&root).expect("read semantic manifest");
    let receipt = build_lsp_dispatch_receipt(&root, &manifest).expect("build receipt");
    (root, manifest, receipt)
}

#[test]
fn lsp_fixture_sequence_records_initialize_and_document_symbols() {
    let (_root, _manifest, receipt) = built_receipt();

    assert_eq!(receipt.schema, LSP_DISPATCH_SCHEMA);
    assert!(receipt
        .fixtures
        .iter()
        .any(|fixture| fixture.method == "initialize"));
    let document_symbol = receipt
        .fixtures
        .iter()
        .find(|fixture| fixture.method == "textDocument/documentSymbol")
        .expect("documentSymbol fixture");
    assert!(document_symbol.observed.document_symbols >= 2);
}

#[test]
fn lsp_fixture_sequence_records_compiler_type_diagnostics() {
    let (_root, _manifest, receipt) = built_receipt();
    let fixture = receipt
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "did-change-type-error")
        .expect("type-error fixture");

    assert_eq!(fixture.method, "textDocument/didChange");
    assert!(fixture.observed.compiler_diagnostics >= 1);
    assert!(fixture.observed.type_errors >= 1);
    assert!(!receipt
        .summary
        .known_gaps
        .contains(&"compiler type-checker diagnostics in LSP".to_string()));
    assert!(receipt
        .summary
        .known_gaps
        .contains(&"full VS Code extension readiness".to_string()));
}

#[test]
fn lsp_fixture_sequence_records_semantic_tokens() {
    let (_root, _manifest, receipt) = built_receipt();
    let fixture = receipt
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "semantic-tokens")
        .expect("semantic token fixture");

    assert_eq!(fixture.method, "textDocument/semanticTokens/full");
    assert!(fixture.observed.semantic_tokens > 0);
}

#[test]
fn lsp_fixture_sequence_records_workspace_symbols() {
    let (_root, _manifest, receipt) = built_receipt();
    let fixture = receipt
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "workspace-symbol")
        .expect("workspace symbol fixture");

    assert_eq!(fixture.method, "workspace/symbol");
    assert!(fixture.observed.workspace_symbols > 0);
}

#[test]
fn lsp_fixture_summary_sorts_methods_and_response_kinds() {
    let (_root, _manifest, receipt) = built_receipt();

    assert!(receipt
        .summary
        .methods
        .windows(2)
        .all(|pair| pair[0] <= pair[1]));
    assert!(receipt
        .summary
        .response_kinds
        .contains(&"response".to_string()));
    assert!(receipt
        .summary
        .response_kinds
        .contains(&"notification".to_string()));
    assert!(receipt.summary.response_kinds.contains(&"none".to_string()));
}

#[test]
fn validate_accepts_current_lsp_dispatch_receipt() {
    let (root, manifest, receipt) = built_receipt();

    validate_lsp_dispatch_receipt(&root, &receipt, &manifest).expect("validate receipt");
}

#[test]
fn validate_rejects_lsp_dispatch_schema_drift() {
    let (root, manifest, mut receipt) = built_receipt();
    receipt.schema = "wrong-schema".to_string();

    let error = validate_lsp_dispatch_receipt(&root, &receipt, &manifest).unwrap_err();

    assert!(error.contains("unsupported schema"));
}

#[test]
fn validate_rejects_lsp_dispatch_fixture_digest_drift() {
    let (root, manifest, mut receipt) = built_receipt();
    receipt
        .fixtures
        .iter_mut()
        .find(|fixture| fixture.id == "document-symbol")
        .expect("document-symbol fixture")
        .result_digest
        .hex = "bad-digest".to_string();

    let error = validate_lsp_dispatch_receipt(&root, &receipt, &manifest).unwrap_err();

    assert!(error.contains("fixture document-symbol result_digest mismatch"));
}

#[test]
fn validate_rejects_lsp_dispatch_observed_drift() {
    let (root, manifest, mut receipt) = built_receipt();
    receipt
        .fixtures
        .iter_mut()
        .find(|fixture| fixture.id == "document-symbol")
        .expect("document-symbol fixture")
        .observed
        .document_symbols = 0;

    let error = validate_lsp_dispatch_receipt(&root, &receipt, &manifest).unwrap_err();

    assert!(error.contains("fixture document-symbol observed drift"));
}

#[test]
fn validate_rejects_lsp_dispatch_summary_drift() {
    let (root, manifest, mut receipt) = built_receipt();
    receipt.summary.known_gaps.push("untracked gap".to_string());

    let error = validate_lsp_dispatch_receipt(&root, &receipt, &manifest).unwrap_err();

    assert!(error.contains("summary drift"));
}
