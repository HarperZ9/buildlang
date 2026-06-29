use std::path::Path;

use super::model::{ModuleGraphProgram, ModuleGraphReceipt};
use super::util::validate_corpus_relative_path;
use super::{build_module_graph_receipt, SemanticCorpusManifest, MODULE_GRAPH_SCHEMA};

pub(crate) fn validate_module_graph_receipt(
    root: &Path,
    receipt: &ModuleGraphReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), String> {
    if receipt.schema != MODULE_GRAPH_SCHEMA {
        return Err(format!(
            "module graph receipt has unsupported schema '{}'",
            receipt.schema
        ));
    }
    if receipt.compiler != "buildc" {
        return Err(format!(
            "module graph compiler mismatch: expected 'buildc', found '{}'",
            receipt.compiler
        ));
    }
    if receipt.language != "buildlang" {
        return Err(format!(
            "module graph language mismatch: expected 'buildlang', found '{}'",
            receipt.language
        ));
    }
    if receipt.source_set.kind != "semantic-corpus" {
        return Err(format!(
            "module graph source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        ));
    }
    let manifest_path =
        validate_corpus_relative_path(root, &receipt.source_set.manifest, "source_set.manifest")?;
    let expected_manifest = root.join("manifest.json").canonicalize().map_err(|err| {
        format!(
            "module graph failed to canonicalize expected manifest {}: {err}",
            root.join("manifest.json").display()
        )
    })?;
    if manifest_path != expected_manifest {
        return Err(format!(
            "module graph source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        ));
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        return Err(format!(
            "module graph source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        ));
    }
    for program in &receipt.programs {
        validate_corpus_relative_path(root, &program.path, "program.path")?;
        for input in &program.inputs {
            validate_corpus_relative_path(root, &input.path, "input.path")?;
        }
    }
    compare_receipts(receipt, &build_module_graph_receipt(root, manifest)?)
}

pub(crate) fn verify_module_graph_receipt(
    root: &Path,
    receipt: &ModuleGraphReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_module_graph_receipt(root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}

fn compare_receipts(
    receipt: &ModuleGraphReceipt,
    expected: &ModuleGraphReceipt,
) -> Result<(), String> {
    if receipt.receipt_id != expected.receipt_id {
        return Err(format!(
            "module graph receipt_id mismatch: expected '{}', found '{}'",
            expected.receipt_id, receipt.receipt_id
        ));
    }
    if receipt.created_at != expected.created_at {
        return Err(format!(
            "module graph created_at mismatch: expected '{}', found '{}'",
            expected.created_at, receipt.created_at
        ));
    }
    if receipt.module_model != expected.module_model {
        return Err("module graph module_model drift".to_string());
    }
    if receipt.programs.len() != expected.programs.len() {
        return Err(format!(
            "module graph program count drift: expected {}, found {}",
            expected.programs.len(),
            receipt.programs.len()
        ));
    }
    for (actual, expected) in receipt.programs.iter().zip(&expected.programs) {
        compare_program(actual, expected)?;
    }
    if receipt.summary != expected.summary {
        return Err("module graph summary drift".to_string());
    }
    Ok(())
}

fn compare_program(
    actual: &ModuleGraphProgram,
    expected: &ModuleGraphProgram,
) -> Result<(), String> {
    if actual.id != expected.id || actual.path != expected.path {
        return Err(format!(
            "module graph program order drift: expected {} at {}, found {} at {}",
            expected.id, expected.path, actual.id, actual.path
        ));
    }
    if actual.source_digest != expected.source_digest {
        return Err(format!(
            "module graph program {} source_digest mismatch",
            actual.id
        ));
    }
    if actual.input_graph_digest != expected.input_graph_digest {
        return Err(format!(
            "module graph program {} input_graph_digest mismatch",
            actual.id
        ));
    }
    if actual.module_graph_digest != expected.module_graph_digest {
        return Err(format!(
            "module graph program {} module_graph_digest mismatch",
            actual.id
        ));
    }
    if actual.inputs != expected.inputs {
        return Err(format!("module graph program {} inputs drift", actual.id));
    }
    if actual.edges != expected.edges {
        return Err(format!("module graph program {} edges drift", actual.id));
    }
    if actual.known_gaps != expected.known_gaps {
        return Err(format!(
            "module graph program {} known_gaps drift",
            actual.id
        ));
    }
    Ok(())
}
