use std::path::Path;

use super::model::SymbolGraphReceipt;
use super::{
    build_symbol_graph_receipt, validate_corpus_relative_path, SemanticCorpusManifest,
    SYMBOL_GRAPH_SCHEMA,
};

pub(crate) fn validate_symbol_graph_receipt(
    root: &Path,
    receipt: &SymbolGraphReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), String> {
    if receipt.schema != SYMBOL_GRAPH_SCHEMA {
        return Err(format!(
            "symbol graph receipt has unsupported schema '{}'",
            receipt.schema
        ));
    }
    if receipt.compiler != "quantac" {
        return Err(format!(
            "symbol graph compiler mismatch: expected 'quantac', found '{}'",
            receipt.compiler
        ));
    }
    if receipt.language != "quantalang" {
        return Err(format!(
            "symbol graph language mismatch: expected 'quantalang', found '{}'",
            receipt.language
        ));
    }
    if receipt.source_set.kind != "semantic-corpus" {
        return Err(format!(
            "symbol graph source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        ));
    }
    let manifest_path =
        validate_corpus_relative_path(root, &receipt.source_set.manifest, "source_set.manifest")?;
    let expected_manifest = root.join("manifest.json").canonicalize().map_err(|err| {
        format!(
            "symbol graph failed to canonicalize expected manifest {}: {err}",
            root.join("manifest.json").display()
        )
    })?;
    if manifest_path != expected_manifest {
        return Err(format!(
            "symbol graph source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        ));
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        return Err(format!(
            "symbol graph source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        ));
    }
    for program in &receipt.programs {
        validate_corpus_relative_path(root, &program.path, "program.path")?;
    }
    compare_receipts(receipt, &build_symbol_graph_receipt(root, manifest)?)
}

pub(crate) fn verify_symbol_graph_receipt(
    root: &Path,
    receipt: &SymbolGraphReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_symbol_graph_receipt(root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}

pub(super) fn compare_receipts(
    receipt: &SymbolGraphReceipt,
    expected: &SymbolGraphReceipt,
) -> Result<(), String> {
    if receipt.receipt_id != expected.receipt_id {
        return Err(format!(
            "symbol graph receipt_id mismatch: expected '{}', found '{}'",
            expected.receipt_id, receipt.receipt_id
        ));
    }
    if receipt.created_at != expected.created_at {
        return Err(format!(
            "symbol graph created_at mismatch: expected '{}', found '{}'",
            expected.created_at, receipt.created_at
        ));
    }
    if receipt.symbol_model != expected.symbol_model {
        return Err("symbol graph symbol_model drift".to_string());
    }
    if receipt.programs.len() != expected.programs.len() {
        return Err(format!(
            "symbol graph program count drift: expected {}, found {}",
            expected.programs.len(),
            receipt.programs.len()
        ));
    }
    for (actual, expected) in receipt.programs.iter().zip(&expected.programs) {
        compare_program(actual, expected)?;
    }
    if receipt.summary != expected.summary {
        return Err("symbol graph summary drift".to_string());
    }
    Ok(())
}

fn compare_program(
    actual: &super::model::SymbolGraphProgram,
    expected: &super::model::SymbolGraphProgram,
) -> Result<(), String> {
    if actual.id != expected.id || actual.path != expected.path {
        return Err(format!(
            "symbol graph program order drift: expected {} at {}, found {} at {}",
            expected.id, expected.path, actual.id, actual.path
        ));
    }
    if actual.source_digest != expected.source_digest {
        return Err(format!(
            "symbol graph program {} source_digest mismatch",
            actual.id
        ));
    }
    if actual.input_graph_digest != expected.input_graph_digest {
        return Err(format!(
            "symbol graph program {} input_graph_digest mismatch",
            actual.id
        ));
    }
    if actual.mir_digest != expected.mir_digest {
        return Err(format!(
            "symbol graph program {} mir_digest mismatch",
            actual.id
        ));
    }
    if actual.symbol_graph_digest != expected.symbol_graph_digest {
        return Err(format!(
            "symbol graph program {} symbol_graph_digest mismatch",
            actual.id
        ));
    }
    if actual.source_symbols != expected.source_symbols {
        return Err(format!(
            "symbol graph program {} source_symbols drift",
            actual.id
        ));
    }
    if actual.mir_symbols != expected.mir_symbols {
        return Err(format!(
            "symbol graph program {} mir_symbols drift",
            actual.id
        ));
    }
    if actual.effect_symbols != expected.effect_symbols {
        return Err(format!(
            "symbol graph program {} effect_symbols drift",
            actual.id
        ));
    }
    if actual.edges != expected.edges {
        return Err(format!("symbol graph program {} edges drift", actual.id));
    }
    if actual.known_gaps != expected.known_gaps {
        return Err(format!(
            "symbol graph program {} known_gaps drift",
            actual.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Component, Path};

    use super::super::model::{SymbolGraphReceipt, SymbolGraphSourceSet};
    use super::super::{
        is_lexically_invalid_relative_path, summarize_programs, symbol_model, SYMBOL_GRAPH_SCHEMA,
    };
    use super::*;

    #[test]
    fn symbol_graph_rejects_lexically_invalid_paths() {
        for path in ["../x.quanta", "C:\\x.quanta", "C:x.quanta", "\\x.quanta"] {
            assert!(
                is_lexically_invalid_relative_path(path)
                    || Path::new(path).components().any(|component| matches!(
                        component,
                        Component::ParentDir | Component::Prefix(_) | Component::RootDir
                    ))
            );
        }
    }

    #[test]
    fn symbol_graph_detects_symbol_model_drift() {
        let expected = SymbolGraphReceipt {
            schema: SYMBOL_GRAPH_SCHEMA.to_string(),
            receipt_id: "id".to_string(),
            created_at: "2026-06-18".to_string(),
            compiler: "quantac".to_string(),
            language: "quantalang".to_string(),
            source_set: SymbolGraphSourceSet {
                kind: "semantic-corpus".to_string(),
                manifest: "manifest.json".to_string(),
                program_count: 0,
            },
            symbol_model: symbol_model(),
            programs: vec![],
            summary: summarize_programs(&[]),
        };
        let mut actual = expected.clone();
        actual.symbol_model.semantic_anchor = "LSP request dispatch verified".to_string();
        assert_eq!(
            compare_receipts(&actual, &expected),
            Err("symbol graph symbol_model drift".to_string())
        );
    }
}
