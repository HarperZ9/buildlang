mod model;
#[cfg(test)]
mod tests;
mod util;
mod validate;

use std::collections::BTreeSet;
use std::path::Path;

use quantalang::lexer::{Lexer, SourceFile};
use quantalang::parser::Parser;

pub(crate) use model::ModuleGraphReceipt;
use model::{
    ModuleGraphDigest, ModuleGraphEdge, ModuleGraphInput, ModuleGraphModel, ModuleGraphProgram,
    ModuleGraphSourceSet, ModuleGraphSummary, ProgramDigestProjection,
};
use util::{
    corpus_relative_path, digest_hex, module_digest, sorted, validate_corpus_relative_path,
};
#[allow(unused_imports)]
pub(crate) use validate::{validate_module_graph_receipt, verify_module_graph_receipt};

use super::{
    input_graph_digest, preprocess_includes_recording_inputs, resolve_imports_recording_inputs,
    resolve_modules_recording_inputs, source_text_digest_hex, CheckReceiptInputDigest,
    InputDigestLedger, SemanticCorpusManifest, SemanticCorpusProgram,
};

pub(crate) const MODULE_GRAPH_RECEIPT: &str = "module-graph-2026-06-18.json";
pub(super) const MODULE_GRAPH_SCHEMA: &str = "quantalang-module-graph-receipt/v0";

fn program_known_gaps() -> Vec<String> {
    sorted(
        [
            "full name resolution is not claimed",
            "package public API graph is not claimed",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    )
}

fn summary_known_gaps() -> Vec<String> {
    sorted(
        [
            "cross-package public API index",
            "full LSP navigation readiness",
            "package registry dependency graph",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    )
}

fn module_model() -> ModuleGraphModel {
    ModuleGraphModel {
        resolver: "quantac source input resolver".to_string(),
        input_roles: vec![
            "entry".to_string(),
            "include".to_string(),
            "import".to_string(),
            "module".to_string(),
        ],
        digest_anchor: "quantalang-check-receipt/v1 input_graph_digest".to_string(),
        symbol_anchor: format!("receipts/{}", crate::symbol_graph::SYMBOL_GRAPH_RECEIPT),
    }
}

fn edge_kind(role: &str) -> Result<&'static str, String> {
    match role {
        "entry" => Ok("program_entry"),
        "include" => Ok("program_include"),
        "import" => Ok("program_import"),
        "module" => Ok("program_module"),
        other => Err(format!("module graph unsupported input role '{other}'")),
    }
}

fn program_digest(program: &ModuleGraphProgram) -> Result<ModuleGraphDigest, String> {
    let projection = ProgramDigestProjection {
        id: &program.id,
        path: &program.path,
        source_digest_hex: &program.source_digest.hex,
        input_graph_digest_hex: &program.input_graph_digest.hex,
        inputs: &program.inputs,
        edges: &program.edges,
        known_gaps: &program.known_gaps,
    };
    serde_json::to_vec(&projection)
        .map(|bytes| module_digest(digest_hex(&bytes)))
        .map_err(|err| format!("module graph failed to encode program digest projection: {err}"))
}

fn source_records(
    program_path: &Path,
) -> Result<(ModuleGraphDigest, Vec<CheckReceiptInputDigest>), String> {
    let entry_bytes = std::fs::read(program_path).map_err(|err| {
        format!(
            "module graph failed to read {}: {err}",
            program_path.display()
        )
    })?;
    let source_digest = module_digest(source_text_digest_hex(&entry_bytes));
    let source = String::from_utf8(entry_bytes).map_err(|err| {
        format!(
            "module graph failed to read UTF-8 source {}: {err}",
            program_path.display()
        )
    })?;
    let base_dir = program_path.parent().unwrap_or_else(|| Path::new("."));
    let mut ledger = InputDigestLedger::text_normalized();
    ledger.record("entry", program_path, source.as_bytes());
    let source =
        resolve_imports_recording_inputs(&source, program_path, &mut ledger).map_err(|_| {
            format!(
                "module graph failed to resolve imports for {}",
                program_path.display()
            )
        })?;
    let source =
        preprocess_includes_recording_inputs(&source, base_dir, &mut ledger).map_err(|_| {
            format!(
                "module graph failed to preprocess includes for {}",
                program_path.display()
            )
        })?;
    let source_file = SourceFile::new(program_path.to_string_lossy(), source);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|err| {
        format!(
            "module graph lexer error in {}: {err}",
            program_path.display()
        )
    })?;
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|err| {
        format!(
            "module graph parse error in {}: {err}",
            program_path.display()
        )
    })?;
    resolve_modules_recording_inputs(&mut ast, base_dir, &mut ledger).map_err(|_| {
        format!(
            "module graph failed to resolve modules for {}",
            program_path.display()
        )
    })?;
    Ok((source_digest, ledger.into_sorted_records()))
}

fn build_inputs(
    root: &Path,
    records: Vec<CheckReceiptInputDigest>,
) -> Result<Vec<ModuleGraphInput>, String> {
    records
        .into_iter()
        .map(|record| {
            let relative = corpus_relative_path(root, Path::new(&record.source), "input.path")?;
            Ok(ModuleGraphInput {
                id: format!("input:{}:{relative}", record.role),
                role: record.role,
                path: relative,
                source_digest: module_digest(record.digest.hex),
            })
        })
        .collect()
}

fn build_edges(
    program_id: &str,
    inputs: &[ModuleGraphInput],
) -> Result<Vec<ModuleGraphEdge>, String> {
    let mut edges = inputs
        .iter()
        .map(|input| {
            Ok(ModuleGraphEdge {
                kind: edge_kind(&input.role)?.to_string(),
                from: format!("program:{program_id}"),
                to: input.id.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    edges.sort();
    Ok(edges)
}

fn build_module_graph_program(
    root: &Path,
    program: &SemanticCorpusProgram,
) -> Result<ModuleGraphProgram, String> {
    let program_path = validate_corpus_relative_path(root, &program.path, "program.path")?;
    let (source_digest, records) = source_records(&program_path)?;
    let input_graph_digest = input_graph_digest(&records);
    let inputs = build_inputs(root, records)?;
    let mut output = ModuleGraphProgram {
        id: program.id.clone(),
        path: program.path.clone(),
        source_digest,
        input_graph_digest: module_digest(input_graph_digest.hex),
        module_graph_digest: module_digest(String::new()),
        inputs,
        edges: Vec::new(),
        known_gaps: program_known_gaps(),
    };
    output.edges = build_edges(&output.id, &output.inputs)?;
    output.module_graph_digest = program_digest(&output)?;
    Ok(output)
}

pub(crate) fn build_module_graph_receipt(
    root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<ModuleGraphReceipt, String> {
    let programs = manifest
        .programs
        .iter()
        .map(|program| build_module_graph_program(root, program))
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize_programs(&programs);
    Ok(ModuleGraphReceipt {
        schema: MODULE_GRAPH_SCHEMA.to_string(),
        receipt_id: "module-graph-semantic-corpus-2026-06-18".to_string(),
        created_at: "2026-06-18".to_string(),
        compiler: "quantac".to_string(),
        language: "quantalang".to_string(),
        source_set: ModuleGraphSourceSet {
            kind: "semantic-corpus".to_string(),
            manifest: "manifest.json".to_string(),
            program_count: manifest.programs.len(),
        },
        module_model: module_model(),
        programs,
        summary,
    })
}

fn summarize_programs(programs: &[ModuleGraphProgram]) -> ModuleGraphSummary {
    let mut input_roles = BTreeSet::new();
    let mut edge_kinds = BTreeSet::new();
    let mut input_count = 0;
    for program in programs {
        input_count += program.inputs.len();
        input_roles.extend(program.inputs.iter().map(|input| input.role.clone()));
        edge_kinds.extend(program.edges.iter().map(|edge| edge.kind.clone()));
    }
    ModuleGraphSummary {
        program_count: programs.len(),
        input_count,
        input_roles: sorted(input_roles),
        edge_kinds: sorted(edge_kinds),
        known_gaps: summary_known_gaps(),
    }
}
