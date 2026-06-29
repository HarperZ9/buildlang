#![allow(dead_code)]

mod collect;
mod model;
mod validate;

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::mir_representation::{
    collect_mir_symbols, digest_mir_module, lower_program_to_mir, MirRepresentationDigest,
};

use collect::{collect_edges, collect_effect_symbols, collect_source_symbols};
pub(crate) use model::SymbolGraphReceipt;
use model::{
    ProgramDigestProjection, SymbolGraphModel, SymbolGraphProgram, SymbolGraphSourceSet,
    SymbolGraphSummary,
};
#[allow(unused_imports)]
pub(crate) use validate::{validate_symbol_graph_receipt, verify_symbol_graph_receipt};

use super::{SemanticCorpusManifest, SemanticCorpusProgram};

pub(crate) const SYMBOL_GRAPH_RECEIPT: &str = "symbol-graph-2026-06-18.json";
pub(super) const SYMBOL_GRAPH_SCHEMA: &str = "buildlang-symbol-graph-receipt/v0";

fn sorted(values: BTreeSet<String>) -> Vec<String> {
    values.into_iter().collect()
}

fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write digest hex");
    }
    hex
}

fn symbol_digest(bytes: &[u8]) -> MirRepresentationDigest {
    MirRepresentationDigest {
        algorithm: "sha256".to_string(),
        hex: digest_hex(bytes),
    }
}

fn is_lexically_invalid_relative_path(relative: &str) -> bool {
    relative.starts_with('\\')
        || relative
            .as_bytes()
            .get(0..2)
            .is_some_and(|bytes| bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

pub(super) fn validate_corpus_relative_path(
    root: &Path,
    relative: &str,
    field: &str,
) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err(format!("symbol graph {field} must not be empty"));
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
            "symbol graph {field} must stay within corpus root: {relative}"
        ));
    }
    let canonical_root = root.canonicalize().map_err(|err| {
        format!(
            "symbol graph {field} failed to canonicalize corpus root {}: {err}",
            root.display()
        )
    })?;
    let path = root.join(relative_path);
    if !path.is_file() {
        return Err(format!(
            "symbol graph {field} path not found: {}",
            path.display()
        ));
    }
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "symbol graph {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "symbol graph {field} must stay within corpus root: {relative}"
        ));
    }
    Ok(canonical_path)
}

fn program_known_gaps() -> Vec<String> {
    sorted(
        [
            "call graph is not claimed",
            "external package resolution is not claimed",
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
            "full LSP request dispatch",
            "full package graph resolution",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    )
}

fn symbol_model() -> SymbolGraphModel {
    SymbolGraphModel {
        source: "AST".to_string(),
        representation: "MIR".to_string(),
        semantic_anchor: "type-checker function effect summaries".to_string(),
        lowering_pipeline: "parse -> type-check -> ast-to-mir".to_string(),
        representation_anchor: format!(
            "receipts/{}",
            crate::mir_representation::MIR_REPRESENTATION_RECEIPT
        ),
        memory_anchor: format!("receipts/{}", crate::memory_layout::MEMORY_LAYOUT_RECEIPT),
    }
}

fn program_digest(program: &SymbolGraphProgram) -> Result<MirRepresentationDigest, String> {
    let projection = ProgramDigestProjection {
        id: &program.id,
        path: &program.path,
        source_digest_hex: &program.source_digest.hex,
        input_graph_digest_hex: &program.input_graph_digest.hex,
        mir_digest_hex: &program.mir_digest.hex,
        source_symbols: &program.source_symbols,
        mir_symbols: &program.mir_symbols,
        effect_symbols: &program.effect_symbols,
        edges: &program.edges,
        known_gaps: &program.known_gaps,
    };
    serde_json::to_vec(&projection)
        .map(|bytes| symbol_digest(&bytes))
        .map_err(|err| format!("symbol graph failed to encode program digest projection: {err}"))
}

fn build_symbol_program(
    root: &Path,
    program: &SemanticCorpusProgram,
) -> Result<SymbolGraphProgram, String> {
    let program_path = validate_corpus_relative_path(root, &program.path, "program.path")?;
    let lowered = lower_program_to_mir(&program_path)?;
    let mut output = SymbolGraphProgram {
        id: program.id.clone(),
        path: program.path.clone(),
        source_digest: lowered.source_digest,
        input_graph_digest: lowered.input_graph_digest,
        mir_digest: digest_mir_module(&lowered.module),
        symbol_graph_digest: MirRepresentationDigest {
            algorithm: "sha256".to_string(),
            hex: String::new(),
        },
        source_symbols: collect_source_symbols(&lowered.ast),
        mir_symbols: collect_mir_symbols(&lowered.module),
        effect_symbols: collect_effect_symbols(&lowered.function_effect_summaries),
        edges: Vec::new(),
        known_gaps: program_known_gaps(),
    };
    output.edges = collect_edges(
        &output.source_symbols,
        &output.mir_symbols,
        &output.effect_symbols,
    );
    output.symbol_graph_digest = program_digest(&output)?;
    Ok(output)
}

pub(crate) fn build_symbol_graph_receipt(
    root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<SymbolGraphReceipt, String> {
    let programs = manifest
        .programs
        .iter()
        .map(|program| build_symbol_program(root, program))
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize_programs(&programs);
    Ok(SymbolGraphReceipt {
        schema: SYMBOL_GRAPH_SCHEMA.to_string(),
        receipt_id: "symbol-graph-semantic-corpus-2026-06-18".to_string(),
        created_at: "2026-06-18".to_string(),
        compiler: "buildc".to_string(),
        language: "buildlang".to_string(),
        source_set: SymbolGraphSourceSet {
            kind: "semantic-corpus".to_string(),
            manifest: "manifest.json".to_string(),
            program_count: manifest.programs.len(),
        },
        symbol_model: symbol_model(),
        programs,
        summary,
    })
}

fn summarize_programs(programs: &[SymbolGraphProgram]) -> SymbolGraphSummary {
    let mut source_kinds = BTreeSet::new();
    let mut mir_kinds = BTreeSet::new();
    let mut effects = BTreeSet::new();
    let mut edge_kinds = BTreeSet::new();
    for program in programs {
        for symbol in &program.source_symbols {
            source_kinds.insert(symbol.kind.clone());
        }
        if !program.mir_symbols.functions.is_empty() {
            mir_kinds.insert("function".to_string());
        }
        if !program.mir_symbols.types.is_empty() {
            mir_kinds.insert("type".to_string());
        }
        if !program.mir_symbols.globals.is_empty() {
            mir_kinds.insert("global".to_string());
        }
        if !program.mir_symbols.externals.is_empty() {
            mir_kinds.insert("external".to_string());
        }
        for effect in &program.effect_symbols {
            effects.extend(effect.declared_effects.iter().cloned());
            effects.extend(effect.observed_capabilities.iter().cloned());
        }
        for edge in &program.edges {
            edge_kinds.insert(edge.kind.clone());
        }
    }
    SymbolGraphSummary {
        program_count: programs.len(),
        source_symbol_kinds: sorted(source_kinds),
        mir_symbol_kinds: sorted(mir_kinds),
        effect_names: sorted(effects),
        edge_kinds: sorted(edge_kinds),
        known_gaps: summary_known_gaps(),
    }
}
