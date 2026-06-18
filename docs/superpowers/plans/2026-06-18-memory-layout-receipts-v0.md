# Memory/RAM Layout Receipts v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a checked Memory/RAM Layout Receipt for the semantic corpus and make `quantac corpus verify` recompute and verify memory evidence from manifest tags plus lowered MIR.

**Architecture:** Keep detailed memory receipt DTOs, classification, digesting, and validation in a focused `compiler/src/memory_layout.rs` module. Reuse the MIR representation lowering/digest path so memory evidence is bound to the same source/input/MIR facts, and keep `compiler/src/main.rs` as the corpus orchestration layer that reads receipts, validates substrate references, and prints receipt status.

**Tech Stack:** Rust 2021, `serde`, `serde_json`, `sha2`, existing `quantac` parser/type-checker/MIR APIs, existing Cargo CLI integration tests.

## Global Constraints

- Schema must be exactly `quantalang-memory-layout-receipt/v0`.
- Checked-in receipt path must be exactly `semantic-corpus/receipts/memory-layout-2026-06-18.json`.
- Substrate receipt reference must be exactly `memory_surface.memory_receipt`.
- Do not add a public memory dump command in this slice.
- Do not add a public memory receipt writer in this slice.
- Do not claim byte-offset ABI layout, allocator/heap proof, async runtime memory proof, or full interprocedural borrow proof.
- Do not promote SPIR-V, LLVM, WASM, x86-64, ARM64, Rust, or any other backend maturity.
- Receipt arrays must be sorted and deduplicated.
- Receipt paths must stay under the semantic corpus root and reject absolute paths, rooted paths, Windows drive or UNC forms, and `..`.
- Tests must be written and observed failing before production code for each behavior.
- Use targeted test slices first; full-suite runs are not required unless implementation touches shared compiler foundations beyond the listed files.

---

## File Structure

- `compiler/tests/cli.rs`: add copied-memory-receipt helper and CLI regressions for valid receipt output, drift rejection, overclaim rejection, and substrate memory receipt path containment.
- `compiler/src/mir_representation.rs`: expose the existing semantic-corpus MIR lowering and digest helpers, and add a focused helper that derives MIR memory surfaces from a `MirModule`.
- `compiler/src/memory_layout.rs`: new module with receipt DTOs, manifest/MIR classifiers, stable memory evidence digesting, receipt building, validation, and unit tests.
- `compiler/src/main.rs`: declare `mod memory_layout;`, import verifier APIs, read the memory receipt in `cmd_corpus_verify`, validate `memory_surface.memory_receipt`, and print `memory layout receipt: ok`.
- `semantic-corpus/receipts/memory-layout-2026-06-18.json`: checked receipt generated from the current semantic corpus by the new builder.
- `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`: add `memory_surface.memory_receipt`.
- `README.md` and `STATUS.md`: document memory layout receipts as representation-level RAM/memory evidence, not ABI or full borrow proof.

## Task 1: Red CLI Contract Tests

**Files:**
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: existing `temp_semantic_corpus(label: &str) -> PathBuf`, `quantac() -> Command`, `repo_root() -> PathBuf`, `c_backend_ready() -> bool`, and `write_substrate_receipt_copy`.
- Produces: `write_memory_layout_receipt_copy(corpus_root: &Path, transform: impl FnOnce(serde_json::Value) -> serde_json::Value)`.

- [ ] **Step 1: Add the copied memory receipt helper**

Add this helper immediately after `write_mir_representation_receipt_copy`:

```rust
fn write_memory_layout_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("memory-layout-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read memory layout receipt"))
            .expect("parse memory layout receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified memory layout receipt");
    fs::write(&receipt_path, format!("{rendered}\n"))
        .expect("write modified memory layout receipt");
}
```

- [ ] **Step 2: Add the valid CLI status test**

Add this test near `corpus_verify_checks_mir_representation_receipt`:

```rust
#[test]
fn corpus_verify_checks_memory_layout_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping memory layout receipt verification because no C backend is available");
        return;
    }

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run quantac corpus verify with memory layout receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept memory layout receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("memory layout receipt: ok"),
        "corpus verify should report memory layout receipt status:\n{}",
        stdout
    );
}
```

- [ ] **Step 3: Add drift and overclaim tests**

Add these tests after the valid memory layout test:

```rust
#[test]
fn corpus_verify_rejects_memory_layout_schema_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_schema");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] =
            serde_json::Value::String("quantalang-memory-layout-receipt/v9".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against bad memory layout schema");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "schema drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout receipt has unsupported schema"),
        "stderr should name memory layout schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_program_count_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_program_count");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout program count drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "program count drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout source_set.program_count mismatch"),
        "stderr should name memory layout program count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_path_escape() {
    let corpus_root = temp_semantic_corpus("memory_layout_path_escape");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.quanta".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "path escape should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory layout program.path must stay within corpus root"),
        "stderr should name memory layout path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_source_digest");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] =
            serde_json::Value::String("0".repeat(64));
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout source digest drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "source digest drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program scalar_branch source_digest mismatch"),
        "stderr should name memory layout source digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_observed_surface_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_observed_surface");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][1]["observed_memory_surfaces"]["references"] =
            serde_json::Value::Bool(false);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout observed surface drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "observed memory surface drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program references_mutation observed_memory_surfaces drift"),
        "stderr should name memory layout observed surface drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_known_gap_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_known_gap");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["known_gaps"] = serde_json::json!(["none"]);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout known gap drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "known gap drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory layout summary drift"),
        "stderr should name memory layout summary drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_byte_layout_overclaim() {
    let corpus_root = temp_semantic_corpus("memory_layout_overclaim");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["memory_model"]["layout_claim"] =
            serde_json::Value::String("byte-offset ABI layout verified".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout overclaim");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "byte layout overclaim should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory layout memory_model drift"),
        "stderr should name memory layout overclaim:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 4: Add substrate memory receipt path containment tests**

Add this focused substrate reference test near the representation receipt containment tests:

```rust
#[test]
fn corpus_verify_rejects_substrate_memory_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_memory_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["memory_surface"]["memory_receipt"] =
            serde_json::Value::String("../memory-layout-2026-06-18.json".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against substrate memory receipt path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "substrate memory receipt path escape should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate memory_surface.memory_receipt must stay within corpus root"),
        "stderr should name substrate memory receipt path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 5: Run red CLI slice**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli memory_layout -- --nocapture
```

Expected: FAIL before implementation because `memory-layout-2026-06-18.json` and verifier wiring do not exist yet.

- [ ] **Step 6: Commit red tests**

```powershell
git add compiler\tests\cli.rs
git commit -m "test: require memory layout receipts"
```

## Task 2: MIR Helper Extraction and Memory Layout Module

**Files:**
- Modify: `compiler/src/mir_representation.rs`
- Create: `compiler/src/memory_layout.rs`

**Interfaces:**
- Consumes: `SemanticCorpusManifest`, manifest program `id/path/surfaces`, and MIR lowering from `mir_representation`.
- Produces:
  - `pub(crate) const MEMORY_LAYOUT_RECEIPT: &str`
  - `pub(crate) struct MemoryLayoutReceipt`
  - `pub(crate) fn build_memory_layout_receipt(corpus_root: &Path, manifest: &SemanticCorpusManifest) -> Result<MemoryLayoutReceipt, String>`
  - `pub(crate) fn validate_memory_layout_receipt(corpus_root: &Path, receipt: &MemoryLayoutReceipt, manifest: &SemanticCorpusManifest) -> Result<(), String>`
  - `pub(crate) fn verify_memory_layout_receipt(corpus_root: &Path, receipt: &MemoryLayoutReceipt, manifest: &SemanticCorpusManifest) -> Result<(), i32>`

- [ ] **Step 1: Expose minimal MIR helpers**

In `compiler/src/mir_representation.rs`, change `LoweredMirProgram`, its fields, `digest_mir_module`, and `lower_program_to_mir` to `pub(crate)`:

```rust
pub(crate) struct LoweredMirProgram {
    pub(crate) source_digest: MirRepresentationDigest,
    pub(crate) input_graph_digest: MirRepresentationDigest,
    pub(crate) module: MirModule,
}

pub(crate) fn digest_mir_module(module: &MirModule) -> MirRepresentationDigest {
    sha256_digest(source_digest_hex(write_mir_module(module).as_bytes()))
}

pub(crate) fn lower_program_to_mir(program_path: &Path) -> Result<LoweredMirProgram, String> {
    // Keep the existing function body unchanged.
}
```

Add this helper near `summarize_mir_program`:

```rust
pub(crate) fn collect_mir_memory_surfaces(
    module: &MirModule,
) -> MirRepresentationMemorySurfaces {
    let mut sets = InventorySets::default();
    let mut memory_surfaces = MirRepresentationMemorySurfaces::default();
    let mut control_flow = MirRepresentationControlFlow::default();

    for function in &module.functions {
        let Some(blocks) = &function.blocks else {
            continue;
        };
        for block in blocks {
            for stmt in &block.stmts {
                collect_stmt(&stmt.kind, &mut sets, &mut memory_surfaces);
            }
            if let Some(terminator) = &block.terminator {
                collect_terminator(
                    terminator,
                    &mut sets,
                    &mut memory_surfaces,
                    &mut control_flow,
                );
            }
        }
    }

    memory_surfaces
}
```

Then replace the manual memory collection block inside `summarize_mir_program` with a call to `collect_mir_memory_surfaces(module)` while preserving operation and control-flow collection for the MIR receipt.

- [ ] **Step 2: Create DTOs and constants**

Create `compiler/src/memory_layout.rs` with the receipt DTOs and schema constants:

```rust
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::mir_representation::{
    collect_mir_memory_surfaces, digest_mir_module, lower_program_to_mir,
    MirRepresentationDigest, MirRepresentationMemorySurfaces, MIR_REPRESENTATION_RECEIPT,
};

use super::{SemanticCorpusManifest, SemanticCorpusProgram};

pub(crate) const MEMORY_LAYOUT_RECEIPT: &str = "memory-layout-2026-06-18.json";
const MEMORY_LAYOUT_SCHEMA: &str = "quantalang-memory-layout-receipt/v0";

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
```

- [ ] **Step 3: Add classification, digest, build, and compare helpers**

In the same file, add implementations that classify manifest tags, active MIR flags, and proof status. The key functions must have these exact names:

```rust
fn sorted(values: BTreeSet<String>) -> Vec<String> {
    values.into_iter().collect()
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
```

Add `build_memory_layout_receipt`, `validate_memory_layout_receipt`, and `verify_memory_layout_receipt` using the same recompute-then-compare pattern as `mir_representation.rs`. `memory_evidence_digest` must digest a stable string containing `id`, `path`, `source_digest.hex`, `input_graph_digest.hex`, `mir_digest.hex`, manifest surface names, active MIR memory names, ownership booleans, layout booleans, and proof-status strings.

- [ ] **Step 4: Add module unit tests**

Add these unit tests in `memory_layout.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
```

- [ ] **Step 5: Run red/passing module slice**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml memory_layout --quiet
cargo test --manifest-path compiler\Cargo.toml mir_representation --quiet
```

Expected: `memory_layout` unit tests pass after module implementation; existing `mir_representation` tests still pass after helper extraction.

- [ ] **Step 6: Commit module work**

```powershell
git add compiler\src\mir_representation.rs compiler\src\memory_layout.rs
git commit -m "feat: build memory layout receipts"
```

## Task 3: Corpus/Substrate Wiring and Checked Receipt

**Files:**
- Modify: `compiler/src/main.rs`
- Modify: `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`
- Create: `semantic-corpus/receipts/memory-layout-2026-06-18.json`

**Interfaces:**
- Consumes: `MEMORY_LAYOUT_RECEIPT`, `MemoryLayoutReceipt`, `verify_memory_layout_receipt`.
- Produces: corpus verification output line `memory layout receipt: ok`.

- [ ] **Step 1: Wire the module into `main.rs`**

Add the module and imports:

```rust
mod memory_layout;

use memory_layout::{
    verify_memory_layout_receipt, MemoryLayoutReceipt, MEMORY_LAYOUT_RECEIPT,
};
```

Extend `cmd_corpus_verify`:

```rust
let memory_receipt_path = receipts_dir.join(MEMORY_LAYOUT_RECEIPT);
let memory_receipt: MemoryLayoutReceipt = read_json(&memory_receipt_path)?;

verify_substrate_receipt(&corpus_root, &substrate_receipt, &manifest)?;
verify_mir_representation_receipt(&corpus_root, &mir_receipt, &manifest)?;
verify_memory_layout_receipt(&corpus_root, &memory_receipt, &manifest)?;
```

Add the success output after the MIR line:

```rust
println!("memory layout receipt: ok");
```

- [ ] **Step 2: Validate the substrate memory receipt path**

Extend `SubstrateMemorySurface`:

```rust
#[derive(serde::Deserialize)]
struct SubstrateMemorySurface {
    ownership_model: String,
    #[serde(default)]
    verified_surfaces: Vec<String>,
    #[serde(default)]
    known_gaps: Vec<String>,
    memory_receipt: String,
}
```

After the current non-empty memory checks in `validate_substrate_receipt`, add:

```rust
let memory_receipt_path = validate_substrate_path(
    corpus_root,
    &receipt.memory_surface.memory_receipt,
    "memory_surface.memory_receipt",
)?;
if memory_receipt_path
    != corpus_root
        .join("receipts")
        .join(MEMORY_LAYOUT_RECEIPT)
        .canonicalize()
        .map_err(|err| {
            format!(
                "substrate memory_surface.memory_receipt failed to canonicalize expected receipt {}: {err}",
                corpus_root.join("receipts").join(MEMORY_LAYOUT_RECEIPT).display()
            )
        })?
{
    return Err(format!(
        "substrate memory_surface.memory_receipt must point at receipts/{}, found {}",
        MEMORY_LAYOUT_RECEIPT, receipt.memory_surface.memory_receipt
    ));
}
```

- [ ] **Step 3: Add the substrate receipt reference**

Update `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`:

```json
"memory_surface": {
  "ownership_model": "rust-inspired",
  "verified_surfaces": [
    "references_mutation",
    "tuple_ownership_reuse",
    "struct_aggregate_reuse",
    "field_assignment_reuse",
    "nested_field_reuse",
    "deref_reuse"
  ],
  "known_gaps": [
    "full interprocedural borrow proof",
    "self-hosted stdlib execution",
    "runtime-linked async execution"
  ],
  "memory_receipt": "receipts/memory-layout-2026-06-18.json"
}
```

- [ ] **Step 4: Generate and check in the memory receipt**

Use a temporary local `println!("{}", serde_json::to_string_pretty(&build_memory_layout_receipt(...).unwrap()).unwrap())` inside the module test only while generating the artifact, run the targeted unit test with `--nocapture`, copy the JSON into `semantic-corpus/receipts/memory-layout-2026-06-18.json`, then remove the temporary print before committing.

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml memory_layout_builds_receipt_for_semantic_corpus -- --nocapture
```

Expected: output contains one valid `quantalang-memory-layout-receipt/v0` JSON object. The committed test must not leave the temporary print in place.

- [ ] **Step 5: Run corpus CLI slice**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli memory_layout -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli substrate -- --nocapture
```

Expected: all three slices pass and valid corpus verification prints `memory layout receipt: ok`.

- [ ] **Step 6: Commit wiring and receipt**

```powershell
git add compiler\src\main.rs semantic-corpus\receipts\substrate-semantic-corpus-2026-06-18.json semantic-corpus\receipts\memory-layout-2026-06-18.json compiler\tests\cli.rs
git commit -m "feat: verify memory layout receipts"
```

## Task 4: Documentation and Evidence Posture

**Files:**
- Modify: `README.md`
- Modify: `STATUS.md`

**Interfaces:**
- Consumes: successful `quantac corpus verify` output and the receipt paths.
- Produces: user-facing documentation that memory layout receipts are representation-level RAM evidence, not ABI layout or full borrow proof.

- [ ] **Step 1: Update README substrate receipt section**

In the section that describes substrate receipts and MIR representation receipts, add this paragraph:

```markdown
The substrate path also carries a checked
`quantalang-memory-layout-receipt/v0` artifact for the semantic corpus. It
recomputes per-program manifest memory tags, MIR-derived memory flags,
ownership-surface classification, layout-scope classification, source/input/MIR
digests, and explicit known gaps during `quantac corpus verify`. This is a
representation-level RAM/memory evidence receipt, not a byte-offset ABI layout
claim, allocator proof, async runtime memory proof, or full interprocedural
borrow proof.
```

- [ ] **Step 2: Update STATUS summary**

In `STATUS.md`, extend the summary paragraph that names the MIR representation receipt with:

```markdown
The same verification path now also checks a
`quantalang-memory-layout-receipt/v0` artifact that binds the corpus memory
surface to manifest tags, MIR-derived memory flags, ownership/layout
classification, digest evidence, and explicit known gaps without claiming
byte-level ABI layout or full borrow proof.
```

- [ ] **Step 3: Run docs-sensitive checks**

Run:

```powershell
git diff --check
rg -n "memory layout receipt|quantalang-memory-layout-receipt/v0|byte-level ABI" README.md STATUS.md
```

Expected: `git diff --check` exits 0 and `rg` shows the new documentation in both files.

- [ ] **Step 4: Commit docs**

```powershell
git add README.md STATUS.md
git commit -m "docs: document memory layout receipts"
```

## Task 5: Final Verification

**Files:**
- Verify only; no planned edits.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: evidence that the memory/RAM receipt layer is implemented and the working tree is clean.

- [ ] **Step 1: Run formatting check**

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
```

Expected: exit 0.

- [ ] **Step 2: Run targeted Rust test slices**

```powershell
cargo test --manifest-path compiler\Cargo.toml memory_layout --quiet
cargo test --manifest-path compiler\Cargo.toml mir_representation --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli memory_layout -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli substrate -- --nocapture
```

Expected: all slices exit 0.

- [ ] **Step 3: Run live corpus verification**

```powershell
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

Expected stdout contains:

```text
Semantic Corpus Verify
manifest: 8 program(s)
c receipt: ok
rust receipt: ok
substrate receipt: ok
mir representation receipt: ok
memory layout receipt: ok
c execution: 8 passed
```

- [ ] **Step 4: Run diff and secret hygiene checks**

```powershell
git diff --check
git check-ignore -v .env .env.local
git diff HEAD~4..HEAD -- . ":(exclude)docs/superpowers/plans/2026-06-18-memory-layout-receipts-v0.md" | rg -n "AKIA|AIza|sk-[A-Za-z0-9]|BEGIN (RSA|OPENSSH|EC|DSA) PRIVATE KEY|password\\s*=|api[_-]?key\\s*=|token\\s*="
```

Expected: `git diff --check` exits 0; `.env` and `.env.local` are ignored; the secret scan exits 1 with no matches.

- [ ] **Step 5: Report final state**

```powershell
git status --short --branch
git log --oneline -5
```

Expected: working tree clean, branch ahead count noted, and the latest commits show tests, feature, docs, and final verification scope.
