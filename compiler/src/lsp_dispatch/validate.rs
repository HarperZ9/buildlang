use std::path::{Component, Path, PathBuf};

use super::model::{LspDispatchFixture, LspDispatchReceipt};
use super::{build_lsp_dispatch_receipt, SemanticCorpusManifest, LSP_DISPATCH_SCHEMA};

pub(crate) fn validate_lsp_dispatch_receipt(
    root: &Path,
    receipt: &LspDispatchReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), String> {
    if receipt.schema != LSP_DISPATCH_SCHEMA {
        return Err(format!(
            "lsp dispatch receipt has unsupported schema '{}'",
            receipt.schema
        ));
    }
    if receipt.compiler != "quantac" {
        return Err(format!(
            "lsp dispatch compiler mismatch: expected 'quantac', found '{}'",
            receipt.compiler
        ));
    }
    if receipt.language != "quantalang" {
        return Err(format!(
            "lsp dispatch language mismatch: expected 'quantalang', found '{}'",
            receipt.language
        ));
    }
    if receipt.source_set.kind != "semantic-corpus" {
        return Err(format!(
            "lsp dispatch source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        ));
    }
    let manifest_path =
        validate_corpus_relative_path(root, &receipt.source_set.manifest, "source_set.manifest")?;
    let expected_manifest = root.join("manifest.json").canonicalize().map_err(|err| {
        format!(
            "lsp dispatch failed to canonicalize expected manifest {}: {err}",
            root.join("manifest.json").display()
        )
    })?;
    if manifest_path != expected_manifest {
        return Err(format!(
            "lsp dispatch source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        ));
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        return Err(format!(
            "lsp dispatch source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        ));
    }
    compare_receipts(receipt, &build_lsp_dispatch_receipt(root, manifest)?)
}

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

fn compare_receipts(
    receipt: &LspDispatchReceipt,
    expected: &LspDispatchReceipt,
) -> Result<(), String> {
    if receipt.receipt_id != expected.receipt_id {
        return Err(format!(
            "lsp dispatch receipt_id mismatch: expected '{}', found '{}'",
            expected.receipt_id, receipt.receipt_id
        ));
    }
    if receipt.created_at != expected.created_at {
        return Err(format!(
            "lsp dispatch created_at mismatch: expected '{}', found '{}'",
            expected.created_at, receipt.created_at
        ));
    }
    if receipt.lsp_model != expected.lsp_model {
        return Err("lsp dispatch lsp_model drift".to_string());
    }
    if receipt.fixtures.len() != expected.fixtures.len() {
        return Err(format!(
            "lsp dispatch fixture count drift: expected {}, found {}",
            expected.fixtures.len(),
            receipt.fixtures.len()
        ));
    }
    for (actual, expected) in receipt.fixtures.iter().zip(&expected.fixtures) {
        compare_fixture(actual, expected)?;
    }
    if receipt.summary != expected.summary {
        return Err("lsp dispatch summary drift".to_string());
    }
    Ok(())
}

fn compare_fixture(
    actual: &LspDispatchFixture,
    expected: &LspDispatchFixture,
) -> Result<(), String> {
    if actual.id != expected.id || actual.method != expected.method {
        return Err(format!(
            "lsp dispatch fixture order drift: expected {} {}, found {} {}",
            expected.id, expected.method, actual.id, actual.method
        ));
    }
    if actual.response_kind != expected.response_kind {
        return Err(format!(
            "lsp dispatch fixture {} response_kind mismatch",
            actual.id
        ));
    }
    if actual.result_digest != expected.result_digest {
        return Err(format!(
            "lsp dispatch fixture {} result_digest mismatch",
            actual.id
        ));
    }
    if actual.observed != expected.observed {
        return Err(format!("lsp dispatch fixture {} observed drift", actual.id));
    }
    Ok(())
}

fn validate_corpus_relative_path(
    root: &Path,
    relative: &str,
    field: &str,
) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err(format!("lsp dispatch {field} must not be empty"));
    }
    let relative_path = Path::new(relative);
    if is_lexically_invalid_relative_path(relative)
        || relative_path.is_absolute()
        || relative_path.has_root()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "lsp dispatch {field} must stay within corpus root: {relative}"
        ));
    }
    let canonical_root = root.canonicalize().map_err(|err| {
        format!(
            "lsp dispatch {field} failed to canonicalize corpus root {}: {err}",
            root.display()
        )
    })?;
    let path = root.join(relative_path);
    if !path.is_file() {
        return Err(format!(
            "lsp dispatch {field} path not found: {}",
            path.display()
        ));
    }
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "lsp dispatch {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "lsp dispatch {field} must stay within corpus root: {relative}"
        ));
    }
    Ok(canonical_path)
}

fn is_lexically_invalid_relative_path(relative: &str) -> bool {
    relative.starts_with('\\')
        || relative
            .as_bytes()
            .get(0..2)
            .is_some_and(|bytes| bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}
