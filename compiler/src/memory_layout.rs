use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::mir_representation::{
    collect_mir_memory_surfaces, digest_mir_module, lower_program_to_mir, MirRepresentationDigest,
    MirRepresentationMemorySurfaces, MIR_REPRESENTATION_RECEIPT,
};

use super::{SemanticCorpusManifest, SemanticCorpusProgram};

pub(crate) const MEMORY_LAYOUT_RECEIPT: &str = "memory-layout-2026-06-18.json";

const MEMORY_LAYOUT_SCHEMA: &str = "quantalang-memory-layout-receipt/v0";
const C_EXECUTION_RECEIPT: &str = "c-execution-2026-06-13.json";

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryLayoutReceipt {
    pub schema: String,
    pub receipt_id: String,
    pub created_at: String,
    pub compiler: String,
    pub language: String,
    pub source_set: MemoryLayoutSourceSet,
    pub memory_model: MemoryLayoutModel,
    pub programs: Vec<MemoryLayoutProgram>,
    pub summary: MemoryLayoutSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryLayoutSourceSet {
    pub kind: String,
    pub manifest: String,
    pub program_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryLayoutModel {
    pub ownership_model: String,
    pub scope: String,
    pub layout_claim: String,
    pub lowering_pipeline: String,
    pub execution_anchor: String,
    pub representation_anchor: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryLayoutProgram {
    pub id: String,
    pub path: String,
    pub source_digest: MirRepresentationDigest,
    pub input_graph_digest: MirRepresentationDigest,
    pub mir_digest: MirRepresentationDigest,
    pub memory_evidence_digest: MirRepresentationDigest,
    pub manifest_surfaces: Vec<String>,
    pub observed_memory_surfaces: MirRepresentationMemorySurfaces,
    pub ownership_surfaces: MemoryOwnershipSurfaces,
    pub layout_surfaces: MemoryLayoutSurfaces,
    pub proof_status: MemoryProofStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryOwnershipSurfaces {
    pub by_value_call: bool,
    pub ownership_reuse: bool,
    pub mutable_struct: bool,
    pub reference_mutation: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryLayoutSurfaces {
    pub struct_fields: bool,
    pub tuple_aggregate: bool,
    pub fixed_array: bool,
    pub nested_field_access: bool,
    pub dereference: bool,
    pub field_assignment: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryProofStatus {
    pub representation_level: String,
    pub execution_level: String,
    pub byte_layout: String,
    pub full_borrow_proof: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MemoryLayoutSummary {
    pub program_count: usize,
    pub manifest_memory_surfaces: Vec<String>,
    pub observed_memory_surfaces: Vec<String>,
    pub verified_surfaces: Vec<String>,
    pub known_gaps: Vec<String>,
}

#[derive(serde::Serialize)]
struct MemoryEvidenceProjection<'a> {
    id: &'a str,
    path: &'a str,
    source_digest_hex: &'a str,
    input_graph_digest_hex: &'a str,
    mir_digest_hex: &'a str,
    manifest_surfaces: &'a [String],
    observed_memory_surfaces: &'a [String],
    ownership_surfaces: &'a MemoryOwnershipSurfaces,
    layout_surfaces: &'a MemoryLayoutSurfaces,
    proof_status: &'a MemoryProofStatus,
}

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

fn memory_layout_digest(hex: String) -> MirRepresentationDigest {
    MirRepresentationDigest {
        algorithm: "sha256".to_string(),
        hex,
    }
}

fn manifest_memory_surfaces(program: &SemanticCorpusProgram) -> Vec<String> {
    let memory_tags = [
        "by-value-call",
        "dereference",
        "field-assignment",
        "fixed-array",
        "immutable-reference",
        "mutable-reference",
        "mutable-struct",
        "nested-field-access",
        "ownership-reuse",
        "struct-fields",
        "tuple-aggregate",
    ];
    program
        .surfaces
        .iter()
        .filter(|surface| memory_tags.contains(&surface.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ownership_surfaces(tags: &[String]) -> MemoryOwnershipSurfaces {
    MemoryOwnershipSurfaces {
        by_value_call: tags.iter().any(|tag| tag == "by-value-call"),
        ownership_reuse: tags.iter().any(|tag| tag == "ownership-reuse"),
        mutable_struct: tags.iter().any(|tag| tag == "mutable-struct"),
        reference_mutation: tags.iter().any(|tag| tag == "mutable-reference"),
    }
}

fn layout_surfaces(tags: &[String]) -> MemoryLayoutSurfaces {
    MemoryLayoutSurfaces {
        struct_fields: tags.iter().any(|tag| tag == "struct-fields"),
        tuple_aggregate: tags.iter().any(|tag| tag == "tuple-aggregate"),
        fixed_array: tags.iter().any(|tag| tag == "fixed-array"),
        nested_field_access: tags.iter().any(|tag| tag == "nested-field-access"),
        dereference: tags.iter().any(|tag| tag == "dereference"),
        field_assignment: tags.iter().any(|tag| tag == "field-assignment"),
    }
}

fn active_memory_surface_names(surfaces: &MirRepresentationMemorySurfaces) -> Vec<String> {
    let mut names = BTreeSet::new();
    for (name, active) in [
        ("aggregate_values", surfaces.aggregate_values),
        ("deref_reads", surfaces.deref_reads),
        ("deref_writes", surfaces.deref_writes),
        ("field_reads", surfaces.field_reads),
        ("field_writes", surfaces.field_writes),
        ("index_reads", surfaces.index_reads),
        ("mutable_references", surfaces.mutable_references),
        ("references", surfaces.references),
    ] {
        if active {
            names.insert(name.to_string());
        }
    }
    sorted(names)
}

fn verified_surfaces() -> Vec<String> {
    sorted(
        [
            "deref_reuse",
            "field_assignment_reuse",
            "nested_field_reuse",
            "references_mutation",
            "struct_aggregate_reuse",
            "tuple_ownership_reuse",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    )
}

fn known_gaps() -> Vec<String> {
    sorted(
        [
            "full interprocedural borrow proof",
            "runtime-linked async execution",
            "self-hosted stdlib execution",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    )
}

fn proof_status() -> MemoryProofStatus {
    MemoryProofStatus {
        representation_level: "verified".to_string(),
        execution_level: "c-stdout-verified".to_string(),
        byte_layout: "not-claimed".to_string(),
        full_borrow_proof: "not-claimed".to_string(),
    }
}

fn memory_model() -> MemoryLayoutModel {
    MemoryLayoutModel {
        ownership_model: "rust-inspired".to_string(),
        scope: "semantic-corpus-mir-memory-surface".to_string(),
        layout_claim: "representation-level memory surface, not byte-offset ABI layout".to_string(),
        lowering_pipeline: "parse -> type-check -> ast-to-mir".to_string(),
        execution_anchor: format!("receipts/{C_EXECUTION_RECEIPT}"),
        representation_anchor: format!("receipts/{MIR_REPRESENTATION_RECEIPT}"),
    }
}

fn memory_evidence_digest(
    id: &str,
    path: &str,
    source_digest: &MirRepresentationDigest,
    input_graph_digest: &MirRepresentationDigest,
    mir_digest: &MirRepresentationDigest,
    manifest_surfaces: &[String],
    observed_memory_surfaces: &MirRepresentationMemorySurfaces,
    ownership_surfaces: &MemoryOwnershipSurfaces,
    layout_surfaces: &MemoryLayoutSurfaces,
    proof_status: &MemoryProofStatus,
) -> Result<MirRepresentationDigest, String> {
    let active_surfaces = active_memory_surface_names(observed_memory_surfaces);
    let projection = MemoryEvidenceProjection {
        id,
        path,
        source_digest_hex: &source_digest.hex,
        input_graph_digest_hex: &input_graph_digest.hex,
        mir_digest_hex: &mir_digest.hex,
        manifest_surfaces,
        observed_memory_surfaces: &active_surfaces,
        ownership_surfaces,
        layout_surfaces,
        proof_status,
    };
    let json = serde_json::to_string(&projection)
        .map_err(|err| format!("memory layout failed to serialize evidence digest: {err}"))?;
    Ok(memory_layout_digest(digest_hex(json.as_bytes())))
}

fn summarize_programs(programs: &[MemoryLayoutProgram]) -> MemoryLayoutSummary {
    let mut manifest_surfaces = BTreeSet::new();
    let mut observed_surfaces = BTreeSet::new();

    for program in programs {
        manifest_surfaces.extend(program.manifest_surfaces.iter().cloned());
        observed_surfaces.extend(active_memory_surface_names(
            &program.observed_memory_surfaces,
        ));
    }

    MemoryLayoutSummary {
        program_count: programs.len(),
        manifest_memory_surfaces: sorted(manifest_surfaces),
        observed_memory_surfaces: sorted(observed_surfaces),
        verified_surfaces: verified_surfaces(),
        known_gaps: known_gaps(),
    }
}

fn summarize_program(
    program: &SemanticCorpusProgram,
    source_digest: MirRepresentationDigest,
    input_graph_digest: MirRepresentationDigest,
    mir_digest: MirRepresentationDigest,
    observed_memory_surfaces: MirRepresentationMemorySurfaces,
) -> Result<MemoryLayoutProgram, String> {
    let manifest_surfaces = manifest_memory_surfaces(program);
    let ownership_surfaces = ownership_surfaces(&manifest_surfaces);
    let layout_surfaces = layout_surfaces(&manifest_surfaces);
    let proof_status = proof_status();
    let memory_evidence_digest = memory_evidence_digest(
        &program.id,
        &program.path,
        &source_digest,
        &input_graph_digest,
        &mir_digest,
        &manifest_surfaces,
        &observed_memory_surfaces,
        &ownership_surfaces,
        &layout_surfaces,
        &proof_status,
    )?;

    Ok(MemoryLayoutProgram {
        id: program.id.clone(),
        path: program.path.clone(),
        source_digest,
        input_graph_digest,
        mir_digest,
        memory_evidence_digest,
        manifest_surfaces,
        observed_memory_surfaces,
        ownership_surfaces,
        layout_surfaces,
        proof_status,
    })
}

pub(crate) fn build_memory_layout_receipt(
    corpus_root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<MemoryLayoutReceipt, String> {
    let mut programs = Vec::new();
    for program in &manifest.programs {
        let program_path =
            validate_corpus_relative_path(corpus_root, &program.path, "program.path")?;
        let lowered = lower_program_to_mir(&program_path)?;
        let mir_digest = digest_mir_module(&lowered.module);
        let observed_memory_surfaces = collect_mir_memory_surfaces(&lowered.module);
        programs.push(summarize_program(
            program,
            lowered.source_digest,
            lowered.input_graph_digest,
            mir_digest,
            observed_memory_surfaces,
        )?);
    }

    let summary = summarize_programs(&programs);
    Ok(MemoryLayoutReceipt {
        schema: MEMORY_LAYOUT_SCHEMA.to_string(),
        receipt_id: "memory-layout-semantic-corpus-2026-06-18".to_string(),
        created_at: "2026-06-18".to_string(),
        compiler: "quantac".to_string(),
        language: "quantalang".to_string(),
        source_set: MemoryLayoutSourceSet {
            kind: "semantic-corpus".to_string(),
            manifest: "manifest.json".to_string(),
            program_count: manifest.programs.len(),
        },
        memory_model: memory_model(),
        programs,
        summary,
    })
}

pub(crate) fn validate_memory_layout_receipt(
    corpus_root: &Path,
    receipt: &MemoryLayoutReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), String> {
    if receipt.schema != MEMORY_LAYOUT_SCHEMA {
        return Err(format!(
            "memory layout receipt has unsupported schema '{}'",
            receipt.schema
        ));
    }
    if receipt.compiler != "quantac" {
        return Err(format!(
            "memory layout compiler mismatch: expected 'quantac', found '{}'",
            receipt.compiler
        ));
    }
    if receipt.language != "quantalang" {
        return Err(format!(
            "memory layout language mismatch: expected 'quantalang', found '{}'",
            receipt.language
        ));
    }
    if receipt.source_set.kind != "semantic-corpus" {
        return Err(format!(
            "memory layout source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        ));
    }
    let manifest_path = validate_corpus_relative_path(
        corpus_root,
        &receipt.source_set.manifest,
        "source_set.manifest",
    )?;
    let expected_manifest = corpus_root
        .join("manifest.json")
        .canonicalize()
        .map_err(|err| {
            format!(
                "memory layout failed to canonicalize expected manifest {}: {err}",
                corpus_root.join("manifest.json").display()
            )
        })?;
    if manifest_path != expected_manifest {
        return Err(format!(
            "memory layout source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        ));
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        return Err(format!(
            "memory layout source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        ));
    }
    for program in &receipt.programs {
        validate_corpus_relative_path(corpus_root, &program.path, "program.path")?;
    }

    let expected = build_memory_layout_receipt(corpus_root, manifest)?;
    compare_receipts(receipt, &expected)
}

pub(crate) fn verify_memory_layout_receipt(
    corpus_root: &Path,
    receipt: &MemoryLayoutReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_memory_layout_receipt(corpus_root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}

fn is_lexically_invalid_relative_path(relative: &str) -> bool {
    if relative.starts_with('\\') {
        return true;
    }

    let bytes = relative.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn validate_corpus_relative_path(
    corpus_root: &Path,
    relative: &str,
    field: &str,
) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err(format!("memory layout {field} must not be empty"));
    }
    if is_lexically_invalid_relative_path(relative) {
        return Err(format!(
            "memory layout {field} must stay within corpus root: {relative}"
        ));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.has_root()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "memory layout {field} must stay within corpus root: {relative}"
        ));
    }
    let canonical_root = corpus_root.canonicalize().map_err(|err| {
        format!(
            "memory layout {field} failed to canonicalize corpus root {}: {err}",
            corpus_root.display()
        )
    })?;
    let path = corpus_root.join(relative_path);
    if !path.is_file() {
        return Err(format!(
            "memory layout {field} path not found: {}",
            path.display()
        ));
    }
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "memory layout {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "memory layout {field} must stay within corpus root: {relative}"
        ));
    }
    Ok(canonical_path)
}

fn compare_receipts(
    receipt: &MemoryLayoutReceipt,
    expected: &MemoryLayoutReceipt,
) -> Result<(), String> {
    if receipt.receipt_id != expected.receipt_id {
        return Err(format!(
            "memory layout receipt_id mismatch: expected '{}', found '{}'",
            expected.receipt_id, receipt.receipt_id
        ));
    }
    if receipt.created_at != expected.created_at {
        return Err(format!(
            "memory layout created_at mismatch: expected '{}', found '{}'",
            expected.created_at, receipt.created_at
        ));
    }
    if receipt.memory_model != expected.memory_model {
        return Err("memory layout memory_model drift".to_string());
    }
    if receipt.programs.len() != expected.programs.len() {
        return Err(format!(
            "memory layout program count drift: expected {}, found {}",
            expected.programs.len(),
            receipt.programs.len()
        ));
    }

    for (actual_program, expected_program) in receipt.programs.iter().zip(&expected.programs) {
        if actual_program.id != expected_program.id || actual_program.path != expected_program.path
        {
            return Err(format!(
                "memory layout program order drift: expected {} at {}, found {} at {}",
                expected_program.id, expected_program.path, actual_program.id, actual_program.path
            ));
        }
        if actual_program.source_digest != expected_program.source_digest {
            return Err(format!(
                "memory layout program {} source_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.input_graph_digest != expected_program.input_graph_digest {
            return Err(format!(
                "memory layout program {} input_graph_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.mir_digest != expected_program.mir_digest {
            return Err(format!(
                "memory layout program {} mir_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.memory_evidence_digest != expected_program.memory_evidence_digest {
            return Err(format!(
                "memory layout program {} memory_evidence_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.manifest_surfaces != expected_program.manifest_surfaces {
            return Err(format!(
                "memory layout program {} manifest_surfaces drift",
                actual_program.id
            ));
        }
        if actual_program.observed_memory_surfaces != expected_program.observed_memory_surfaces {
            return Err(format!(
                "memory layout program {} observed_memory_surfaces drift",
                actual_program.id
            ));
        }
        if actual_program.ownership_surfaces != expected_program.ownership_surfaces {
            return Err(format!(
                "memory layout program {} ownership_surfaces drift",
                actual_program.id
            ));
        }
        if actual_program.layout_surfaces != expected_program.layout_surfaces {
            return Err(format!(
                "memory layout program {} layout_surfaces drift",
                actual_program.id
            ));
        }
        if actual_program.proof_status != expected_program.proof_status {
            return Err(format!(
                "memory layout program {} proof_status drift",
                actual_program.id
            ));
        }
    }

    if receipt.summary != expected.summary {
        return Err("memory layout summary drift".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_layout_builds_receipt_for_semantic_corpus() {
        let corpus_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("compiler manifest has repository parent")
            .join("semantic-corpus");
        let manifest: SemanticCorpusManifest =
            serde_json::from_slice(&std::fs::read(corpus_root.join("manifest.json")).unwrap())
                .unwrap();

        let receipt =
            build_memory_layout_receipt(&corpus_root, &manifest).expect("build memory receipt");

        assert_eq!(receipt.schema, MEMORY_LAYOUT_SCHEMA);
        assert_eq!(receipt.source_set.program_count, manifest.programs.len());
        assert_eq!(receipt.programs.len(), manifest.programs.len());
        assert!(receipt
            .summary
            .observed_memory_surfaces
            .contains(&"references".to_string()));
        assert_eq!(
            receipt.memory_model.layout_claim,
            memory_model().layout_claim
        );
    }

    #[test]
    fn memory_layout_manifest_surfaces_are_sorted_and_deduplicated() {
        let program = SemanticCorpusProgram {
            id: "p".to_string(),
            path: "programs/p.quanta".to_string(),
            surfaces: vec![
                "stdout".to_string(),
                "ownership-reuse".to_string(),
                "mutable-reference".to_string(),
                "ownership-reuse".to_string(),
                "struct-fields".to_string(),
            ],
            expected_stdout: String::new(),
        };

        assert_eq!(
            manifest_memory_surfaces(&program),
            vec!["mutable-reference", "ownership-reuse", "struct-fields"]
        );
    }

    #[test]
    fn memory_layout_classifies_manifest_surfaces() {
        let tags = vec![
            "by-value-call".to_string(),
            "ownership-reuse".to_string(),
            "mutable-struct".to_string(),
            "field-assignment".to_string(),
            "dereference".to_string(),
        ];

        let ownership = ownership_surfaces(&tags);
        assert!(ownership.by_value_call);
        assert!(ownership.ownership_reuse);
        assert!(ownership.mutable_struct);
        assert!(!ownership.reference_mutation);

        let layout = layout_surfaces(&tags);
        assert!(layout.field_assignment);
        assert!(layout.dereference);
        assert!(!layout.fixed_array);
    }

    #[test]
    fn memory_layout_active_mir_surfaces_are_sorted() {
        let surfaces = MirRepresentationMemorySurfaces {
            references: true,
            mutable_references: true,
            deref_reads: false,
            deref_writes: true,
            field_reads: true,
            field_writes: false,
            index_reads: false,
            aggregate_values: true,
        };

        assert_eq!(
            active_memory_surface_names(&surfaces),
            vec![
                "aggregate_values",
                "deref_writes",
                "field_reads",
                "mutable_references",
                "references",
            ]
        );
    }
}
