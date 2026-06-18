# MIR Representation Receipts v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a checked MIR representation receipt for the semantic corpus and make `quantac corpus verify` recompute and verify it from the real parse, type-check, and AST-to-MIR lowering pipeline.

**Architecture:** Keep the inventory builder and verifier in a focused binary-crate module, `compiler/src/mir_representation.rs`. `compiler/src/main.rs` remains the corpus orchestration layer: it reads the receipt, delegates MIR validation, validates the substrate reference path, and prints the receipt status. CLI tests mutate copied semantic-corpus fixtures to prove stale representation claims are rejected.

**Tech Stack:** Rust 2021, `serde`, `serde_json`, `sha2`, existing `quantac` parser/type-checker/codegen APIs, existing Cargo CLI integration tests.

## Global Constraints

- Schema must be exactly `quantalang-mir-representation-receipt/v0`.
- Checked-in receipt path must be exactly `semantic-corpus/receipts/mir-representation-2026-06-18.json`.
- Substrate receipt reference must be exactly `representation_surface.representation_receipt`.
- Do not add a public `quantac mir dump` command in this slice.
- Do not add a public representation receipt writer in this slice.
- Do not promote SPIR-V, LLVM, WASM, x86-64, ARM64, or Rust backend maturity.
- Receipt arrays must be sorted and deduplicated.
- Receipt paths must stay under the semantic corpus root and reject absolute paths or `..`.
- Tests must be written and observed failing before production code for each behavior.
- Use targeted test slices first; full-suite runs are not required unless the change touches shared compiler foundations beyond the listed files.

---

## File Structure

- `compiler/tests/cli.rs`: add corpus-copy helper and CLI regression tests for valid and invalid MIR representation receipts.
- `compiler/src/mir_representation.rs`: new focused module containing receipt DTOs, MIR inventory construction, receipt recomputation, validation, and unit tests.
- `compiler/src/main.rs`: declare `mod mir_representation;`, import verifier APIs, wire `cmd_corpus_verify`, expand `SubstrateRepresentationSurface`, and validate the substrate receipt's representation receipt path.
- `semantic-corpus/receipts/mir-representation-2026-06-18.json`: checked-in receipt generated from the current semantic corpus by the new builder.
- `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`: add `representation_surface.representation_receipt`.
- `README.md` and `STATUS.md`: document MIR Representation Receipts as representation evidence, not backend promotion.

## Task 1: Red CLI Contract Tests

**Files:**
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: existing `temp_semantic_corpus(label: &str) -> PathBuf`, `quantac() -> Command`, `repo_root() -> PathBuf`, and `c_backend_ready() -> bool`.
- Produces: `write_mir_representation_receipt_copy(corpus_root: &Path, transform: impl FnOnce(serde_json::Value) -> serde_json::Value)` for later test tasks.

- [ ] **Step 1: Add the copied-receipt helper**

Add this helper immediately after `write_substrate_receipt_copy`:

```rust
fn write_mir_representation_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("mir-representation-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read MIR representation receipt"))
            .expect("parse MIR representation receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified MIR representation receipt");
    fs::write(&receipt_path, format!("{rendered}\n"))
        .expect("write modified MIR representation receipt");
}
```

- [ ] **Step 2: Add the valid CLI test**

Add this test near `corpus_verify_checks_substrate_receipt`:

```rust
#[test]
fn corpus_verify_checks_mir_representation_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping MIR representation receipt verification because no C backend is available");
        return;
    }

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run quantac corpus verify with MIR representation receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept MIR representation receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("mir representation receipt: ok"),
        "corpus verify should report MIR representation receipt status:\n{}",
        stdout
    );
}
```

- [ ] **Step 3: Add invalid receipt tests**

Add these tests after the valid MIR representation test:

```rust
#[test]
fn corpus_verify_rejects_mir_representation_receipt_schema_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_schema");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] =
            serde_json::Value::String("quantalang-mir-representation-receipt/v9".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against bad MIR representation schema");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation schema drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation receipt has unsupported schema"),
        "stderr should name MIR representation schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_program_count_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_program_count");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation program count drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation program count drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation source_set.program_count mismatch"),
        "stderr should name MIR representation program count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_path_escape() {
    let corpus_root = temp_semantic_corpus("mir_repr_path_escape");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.quanta".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name MIR representation path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_source_digest");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation source digest drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation source digest drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch source_digest mismatch"),
        "stderr should name MIR representation source digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_operation_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_operation_drift");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["operations"]["rvalues"] =
            serde_json::json!(["ForgedRValue"]);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation operation drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation operation drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch operations.rvalues drift"),
        "stderr should name MIR representation operation drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 4: Run the red CLI slice**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli mir_representation -- --nocapture
```

Expected: FAIL. The helper compiles only after the checked-in receipt path exists in copied corpus fixtures; the valid test also fails because `quantac corpus verify` does not print `mir representation receipt: ok`.

- [ ] **Step 5: Commit the red tests**

Run:

```powershell
git add compiler\tests\cli.rs
git commit -m "test: require mir representation receipts"
```

## Task 2: MIR Representation Inventory Module

**Files:**
- Create: `compiler/src/mir_representation.rs`
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `super::SemanticCorpusManifest`, `super::source_digest_hex(bytes: &[u8]) -> String`, `super::resolve_imports(source: &str, file: &Path) -> Result<String, i32>`, `super::preprocess_includes(source: &str, base_dir: &Path) -> Result<String, i32>`, `super::resolve_modules(ast: &mut ast::Module, source_dir: &Path) -> Result<(), i32>`.
- Produces:
  - `pub(crate) const MIR_REPRESENTATION_RECEIPT: &str`
  - `pub(crate) fn build_mir_representation_receipt(corpus_root: &Path, manifest: &SemanticCorpusManifest) -> Result<MirRepresentationReceipt, String>`
  - `pub(crate) fn validate_mir_representation_receipt(corpus_root: &Path, receipt: &MirRepresentationReceipt, manifest: &SemanticCorpusManifest) -> Result<(), String>`
  - `pub(crate) fn verify_mir_representation_receipt(corpus_root: &Path, receipt: &MirRepresentationReceipt, manifest: &SemanticCorpusManifest) -> Result<(), i32>`

- [ ] **Step 1: Wire the module name**

At the top of `compiler/src/main.rs`, below the crate documentation imports and before `use clap`, add:

```rust
mod mir_representation;
```

After the existing `use quantalang::types::{...};` block, add:

```rust
use mir_representation::{
    verify_mir_representation_receipt, MirRepresentationReceipt, MIR_REPRESENTATION_RECEIPT,
};
```

- [ ] **Step 2: Create the module with DTOs and inventory helpers**

Create `compiler/src/mir_representation.rs` with this initial structure:

```rust
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use quantalang::ast;
use quantalang::codegen::{
    AggregateKind, BinOp, CastKind, CodeGenerator, MirModule, MirPlace, MirRValue, MirStmtKind,
    MirTerminator, MirType, PlaceProjection, Target, UnaryOp,
};
use quantalang::lexer::{Lexer, SourceFile};
use quantalang::parser::Parser;
use quantalang::types::{TypeChecker, TypeContext};

use super::{
    preprocess_includes, resolve_imports, resolve_modules, source_digest_hex,
    SemanticCorpusManifest,
};

pub(crate) const MIR_REPRESENTATION_RECEIPT: &str =
    "mir-representation-2026-06-18.json";

const MIR_REPRESENTATION_SCHEMA: &str = "quantalang-mir-representation-receipt/v0";

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationReceipt {
    pub schema: String,
    pub receipt_id: String,
    pub created_at: String,
    pub compiler: String,
    pub language: String,
    pub source_set: MirRepresentationSourceSet,
    pub ir: MirRepresentationIr,
    pub programs: Vec<MirRepresentationProgram>,
    pub summary: MirRepresentationSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationSourceSet {
    pub kind: String,
    pub manifest: String,
    pub program_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationIr {
    pub name: String,
    pub version: String,
    pub lowering_pipeline: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationDigest {
    pub algorithm: String,
    pub hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationProgram {
    pub id: String,
    pub path: String,
    pub source_digest: MirRepresentationDigest,
    pub module: MirRepresentationModuleCounts,
    pub symbols: MirRepresentationSymbols,
    pub operations: MirRepresentationOperations,
    pub memory_surfaces: MirRepresentationMemorySurfaces,
    pub control_flow: MirRepresentationControlFlow,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationModuleCounts {
    pub function_count: usize,
    pub defined_function_count: usize,
    pub declaration_count: usize,
    pub type_count: usize,
    pub global_count: usize,
    pub string_count: usize,
    pub external_count: usize,
    pub vtable_count: usize,
    pub uniform_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationSymbols {
    pub functions: Vec<String>,
    pub types: Vec<String>,
    pub globals: Vec<String>,
    pub externals: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationOperations {
    pub statements: Vec<String>,
    pub rvalues: Vec<String>,
    pub terminators: Vec<String>,
    pub binary_ops: Vec<String>,
    pub unary_ops: Vec<String>,
    pub casts: Vec<String>,
    pub aggregate_kinds: Vec<String>,
    pub place_projections: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationMemorySurfaces {
    pub references: bool,
    pub mutable_references: bool,
    pub deref_reads: bool,
    pub deref_writes: bool,
    pub field_reads: bool,
    pub field_writes: bool,
    pub index_reads: bool,
    pub aggregate_values: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationControlFlow {
    pub block_count: usize,
    pub branching: bool,
    pub switching: bool,
    pub calls: bool,
    pub loops: bool,
    pub unreachable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct MirRepresentationSummary {
    pub program_count: usize,
    pub statement_families: Vec<String>,
    pub rvalue_families: Vec<String>,
    pub terminator_families: Vec<String>,
    pub memory_surfaces: Vec<String>,
}

#[derive(Default)]
struct InventorySets {
    statements: BTreeSet<String>,
    rvalues: BTreeSet<String>,
    terminators: BTreeSet<String>,
    binary_ops: BTreeSet<String>,
    unary_ops: BTreeSet<String>,
    casts: BTreeSet<String>,
    aggregate_kinds: BTreeSet<String>,
    place_projections: BTreeSet<String>,
}

impl InventorySets {
    fn operations(self) -> MirRepresentationOperations {
        MirRepresentationOperations {
            statements: sorted(self.statements),
            rvalues: sorted(self.rvalues),
            terminators: sorted(self.terminators),
            binary_ops: sorted(self.binary_ops),
            unary_ops: sorted(self.unary_ops),
            casts: sorted(self.casts),
            aggregate_kinds: sorted(self.aggregate_kinds),
            place_projections: sorted(self.place_projections),
        }
    }
}

fn sorted(values: BTreeSet<String>) -> Vec<String> {
    values.into_iter().collect()
}
```

- [ ] **Step 3: Add unit tests first**

At the bottom of `compiler/src/mir_representation.rs`, add these tests before implementing full traversal:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use quantalang::codegen::{
        AggregateKind, BinOp, BlockId, MirBlock, MirConst, MirFnSig, MirFunction, MirLocal,
        MirStmt, MirValue,
    };

    #[test]
    fn mir_representation_summary_sorts_and_deduplicates_families() {
        let programs = vec![
            MirRepresentationProgram {
                id: "b".to_string(),
                path: "programs/b.quanta".to_string(),
                source_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "1".repeat(64),
                },
                module: MirRepresentationModuleCounts::default(),
                symbols: MirRepresentationSymbols::default(),
                operations: MirRepresentationOperations {
                    statements: vec!["FieldAssign".to_string(), "Assign".to_string()],
                    rvalues: vec!["Use".to_string(), "BinaryOp".to_string()],
                    terminators: vec!["Return".to_string()],
                    binary_ops: Vec::new(),
                    unary_ops: Vec::new(),
                    casts: Vec::new(),
                    aggregate_kinds: Vec::new(),
                    place_projections: Vec::new(),
                },
                memory_surfaces: MirRepresentationMemorySurfaces {
                    field_writes: true,
                    ..Default::default()
                },
                control_flow: MirRepresentationControlFlow::default(),
            },
            MirRepresentationProgram {
                id: "a".to_string(),
                path: "programs/a.quanta".to_string(),
                source_digest: MirRepresentationDigest {
                    algorithm: "sha256".to_string(),
                    hex: "2".repeat(64),
                },
                module: MirRepresentationModuleCounts::default(),
                symbols: MirRepresentationSymbols::default(),
                operations: MirRepresentationOperations {
                    statements: vec!["Assign".to_string()],
                    rvalues: vec!["Use".to_string()],
                    terminators: vec!["If".to_string()],
                    binary_ops: Vec::new(),
                    unary_ops: Vec::new(),
                    casts: Vec::new(),
                    aggregate_kinds: Vec::new(),
                    place_projections: Vec::new(),
                },
                memory_surfaces: MirRepresentationMemorySurfaces {
                    references: true,
                    ..Default::default()
                },
                control_flow: MirRepresentationControlFlow::default(),
            },
        ];

        let summary = summarize_programs(&programs);

        assert_eq!(summary.program_count, 2);
        assert_eq!(summary.statement_families, vec!["Assign", "FieldAssign"]);
        assert_eq!(summary.rvalue_families, vec!["BinaryOp", "Use"]);
        assert_eq!(summary.terminator_families, vec!["If", "Return"]);
        assert_eq!(summary.memory_surfaces, vec!["field_writes", "references"]);
    }

    #[test]
    fn mir_representation_memory_surfaces_derive_from_mir() {
        let mut module = MirModule::new("memory_test");
        module.add_type(quantalang_type_def_point());

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut func = MirFunction::new("main", sig);
        func.add_local(MirLocal::new(LocalId(0), MirType::Struct(Arc::from("Point"))));
        func.add_local(MirLocal::new(
            LocalId(1),
            MirType::Ptr(Box::new(MirType::Struct(Arc::from("Point")))),
        ));
        let mut block = MirBlock::new(BlockId::ENTRY);
        block.push_stmt(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: true,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        block.push_stmt(MirStmt::assign(
            LocalId(0),
            MirRValue::Aggregate {
                kind: AggregateKind::Struct(Arc::from("Point")),
                operands: vec![MirValue::Const(MirConst::Int(1, MirType::i32()))],
            },
        ));
        block.push_stmt(MirStmt::assign(
            LocalId(0),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("x"),
                field_ty: MirType::i32(),
            },
        ));
        block.push_stmt(MirStmt::new(MirStmtKind::DerefAssign {
            ptr: LocalId(1),
            value: MirRValue::Use(MirValue::Const(MirConst::Int(2, MirType::i32()))),
        }));
        block.set_terminator(MirTerminator::Return(None));
        func.add_block(block);
        module.add_function(func);

        let program = summarize_mir_program(
            "memory_test",
            "programs/memory_test.quanta",
            MirRepresentationDigest {
                algorithm: "sha256".to_string(),
                hex: "3".repeat(64),
            },
            &module,
        );

        assert!(program.memory_surfaces.references);
        assert!(program.memory_surfaces.mutable_references);
        assert!(program.memory_surfaces.deref_writes);
        assert!(program.memory_surfaces.field_reads);
        assert!(program.memory_surfaces.aggregate_values);
        assert_eq!(
            program.operations.rvalues,
            vec!["Aggregate", "FieldAccess", "Ref", "Use"]
        );
    }

    fn quantalang_type_def_point() -> quantalang::codegen::MirTypeDef {
        quantalang::codegen::MirTypeDef {
            name: Arc::from("Point"),
            kind: quantalang::codegen::TypeDefKind::Struct {
                fields: vec![(Some(Arc::from("x")), MirType::i32())],
                packed: false,
            },
        }
    }
}
```

- [ ] **Step 4: Run unit tests and confirm RED**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml mir_representation --quiet
```

Expected: FAIL with missing functions such as `summarize_programs` and `summarize_mir_program`.

- [ ] **Step 5: Implement summarization helpers**

Add these public and private helpers in `compiler/src/mir_representation.rs`:

```rust
fn summarize_programs(programs: &[MirRepresentationProgram]) -> MirRepresentationSummary {
    let mut statement_families = BTreeSet::new();
    let mut rvalue_families = BTreeSet::new();
    let mut terminator_families = BTreeSet::new();
    let mut memory_surfaces = BTreeSet::new();

    for program in programs {
        statement_families.extend(program.operations.statements.iter().cloned());
        rvalue_families.extend(program.operations.rvalues.iter().cloned());
        terminator_families.extend(program.operations.terminators.iter().cloned());
        push_memory_surface_names(&mut memory_surfaces, &program.memory_surfaces);
    }

    MirRepresentationSummary {
        program_count: programs.len(),
        statement_families: sorted(statement_families),
        rvalue_families: sorted(rvalue_families),
        terminator_families: sorted(terminator_families),
        memory_surfaces: sorted(memory_surfaces),
    }
}

fn push_memory_surface_names(
    output: &mut BTreeSet<String>,
    surfaces: &MirRepresentationMemorySurfaces,
) {
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
            output.insert(name.to_string());
        }
    }
}

fn summarize_mir_program(
    id: &str,
    path: &str,
    source_digest: MirRepresentationDigest,
    module: &MirModule,
) -> MirRepresentationProgram {
    let module_counts = MirRepresentationModuleCounts {
        function_count: module.functions.len(),
        defined_function_count: module
            .functions
            .iter()
            .filter(|function| !function.is_declaration())
            .count(),
        declaration_count: module
            .functions
            .iter()
            .filter(|function| function.is_declaration())
            .count(),
        type_count: module.types.len(),
        global_count: module.globals.len(),
        string_count: module.strings.len(),
        external_count: module.externals.len(),
        vtable_count: module.vtables.len(),
        uniform_count: module.uniforms.len(),
    };
    let symbols = MirRepresentationSymbols {
        functions: sorted(
            module
                .functions
                .iter()
                .map(|function| function.name.to_string())
                .collect(),
        ),
        types: sorted(module.types.iter().map(|ty| ty.name.to_string()).collect()),
        globals: sorted(module.globals.iter().map(|global| global.name.to_string()).collect()),
        externals: sorted(module.externals.iter().map(|external| external.name.to_string()).collect()),
    };

    let mut sets = InventorySets::default();
    let mut memory_surfaces = MirRepresentationMemorySurfaces::default();
    let mut control_flow = MirRepresentationControlFlow::default();

    for function in &module.functions {
        let Some(blocks) = &function.blocks else {
            continue;
        };
        control_flow.block_count += blocks.len();
        for block in blocks {
            for stmt in &block.stmts {
                collect_stmt(&stmt.kind, &mut sets, &mut memory_surfaces);
            }
            if let Some(terminator) = &block.terminator {
                collect_terminator(terminator, &mut sets, &mut memory_surfaces, &mut control_flow);
            }
        }
    }

    MirRepresentationProgram {
        id: id.to_string(),
        path: path.to_string(),
        source_digest,
        module: module_counts,
        symbols,
        operations: sets.operations(),
        memory_surfaces,
        control_flow,
    }
}
```

Add `collect_stmt`, `collect_rvalue`, `collect_place`, `collect_terminator`, and family-name helpers with exhaustive matches over the MIR enum variants in `compiler/src/codegen/ir.rs`. This includes the current terminator families `Goto`, `If`, `Switch`, `Call`, `Return`, `Unreachable`, `Drop`, `Assert`, `Resume`, and `Abort`. The helper signatures must be:

```rust
fn collect_stmt(
    stmt: &MirStmtKind,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
)

fn collect_rvalue(
    rvalue: &MirRValue,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
)

fn collect_place(
    place: &MirPlace,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
)

fn collect_terminator(
    terminator: &MirTerminator,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
    control_flow: &mut MirRepresentationControlFlow,
)
```

The family-name helpers must return these exact strings:

```rust
fn bin_op_name(op: BinOp) -> &'static str
fn unary_op_name(op: UnaryOp) -> &'static str
fn cast_kind_name(kind: CastKind) -> &'static str
fn aggregate_kind_name(kind: &AggregateKind) -> &'static str
fn projection_name(projection: &PlaceProjection) -> &'static str
```

- [ ] **Step 6: Run unit tests and confirm GREEN**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml mir_representation --quiet
```

Expected: PASS.

- [ ] **Step 7: Commit the module skeleton and inventory helpers**

Run:

```powershell
git add compiler\src\main.rs compiler\src\mir_representation.rs
git commit -m "feat: summarize mir representation inventory"
```

## Task 3: Receipt Builder and Verifier

**Files:**
- Modify: `compiler/src/mir_representation.rs`
- Modify: `compiler/src/main.rs`
- Create: `semantic-corpus/receipts/mir-representation-2026-06-18.json`

**Interfaces:**
- Consumes: Task 2 DTOs and `summarize_mir_program`.
- Produces: corpus verifier integration and checked-in MIR representation receipt.

- [ ] **Step 1: Add builder tests first**

Add this unit test in `compiler/src/mir_representation.rs`:

```rust
#[test]
fn mir_representation_builds_receipt_for_semantic_corpus() {
    let corpus_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest has repository parent")
        .join("semantic-corpus");
    let manifest: SemanticCorpusManifest =
        serde_json::from_slice(&std::fs::read(corpus_root.join("manifest.json")).unwrap())
            .unwrap();

    let receipt = build_mir_representation_receipt(&corpus_root, &manifest)
        .expect("build MIR representation receipt");

    assert_eq!(receipt.schema, MIR_REPRESENTATION_SCHEMA);
    assert_eq!(receipt.source_set.program_count, manifest.programs.len());
    assert_eq!(receipt.programs.len(), manifest.programs.len());
    assert_eq!(receipt.summary.program_count, manifest.programs.len());
    assert_eq!(receipt.programs[0].id, manifest.programs[0].id);
    assert_eq!(receipt.programs[0].path, manifest.programs[0].path);
    assert_eq!(receipt.programs[0].source_digest.algorithm, "sha256");
    assert_eq!(receipt.programs[0].source_digest.hex.len(), 64);
}
```

- [ ] **Step 2: Run builder test and confirm RED**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml mir_representation_builds_receipt_for_semantic_corpus --quiet
```

Expected: FAIL because `build_mir_representation_receipt` does not compile or does not lower corpus programs yet.

- [ ] **Step 3: Implement corpus path validation and lowering**

Add these helpers in `compiler/src/mir_representation.rs`:

```rust
fn validate_corpus_relative_path(
    corpus_root: &Path,
    relative: &str,
    field: &str,
) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err(format!("mir representation {field} must not be empty"));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "mir representation {field} must stay within corpus root: {relative}"
        ));
    }
    let canonical_root = corpus_root.canonicalize().map_err(|err| {
        format!(
            "mir representation {field} failed to canonicalize corpus root {}: {err}",
            corpus_root.display()
        )
    })?;
    let path = corpus_root.join(relative_path);
    if !path.is_file() {
        return Err(format!(
            "mir representation {field} path not found: {}",
            path.display()
        ));
    }
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "mir representation {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "mir representation {field} must stay within corpus root: {relative}"
        ));
    }
    Ok(canonical_path)
}

fn lower_program_to_mir(program_path: &Path) -> Result<MirModule, String> {
    let source = std::fs::read_to_string(program_path).map_err(|err| {
        format!(
            "mir representation failed to read {}: {err}",
            program_path.display()
        )
    })?;
    let source = resolve_imports(&source, program_path)
        .map_err(|_| format!("mir representation failed to resolve imports for {}", program_path.display()))?;
    let base_dir = program_path.parent().unwrap_or_else(|| Path::new("."));
    let source = preprocess_includes(&source, base_dir).map_err(|_| {
        format!(
            "mir representation failed to preprocess includes for {}",
            program_path.display()
        )
    })?;
    let source_file = SourceFile::new(program_path.to_string_lossy(), source);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer
        .tokenize()
        .map_err(|err| format!("mir representation lexer error in {}: {err}", program_path.display()))?;
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|err| {
        format!(
            "mir representation parse error in {}: {err}",
            program_path.display()
        )
    })?;
    if !parser.errors().is_empty() {
        let errors = parser
            .errors()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "mir representation parse errors in {}: {errors}",
            program_path.display()
        ));
    }
    resolve_modules(&mut ast, base_dir).map_err(|_| {
        format!(
            "mir representation failed to resolve modules for {}",
            program_path.display()
        )
    })?;

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.set_source_dir(base_dir.to_path_buf());
    checker.check_module(&ast);
    if checker.has_errors() {
        let errors = checker
            .errors()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "mir representation type errors in {}: {errors}",
            program_path.display()
        ));
    }

    let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source_file.source()));
    codegen.generate(&ast).map_err(|err| {
        format!(
            "mir representation code generation error in {}: {err}",
            program_path.display()
        )
    })?;
    codegen
        .mir()
        .cloned()
        .ok_or_else(|| format!("mir representation did not produce MIR for {}", program_path.display()))
}
```

- [ ] **Step 4: Implement receipt construction**

Add this function:

```rust
pub(crate) fn build_mir_representation_receipt(
    corpus_root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<MirRepresentationReceipt, String> {
    let mut programs = Vec::new();
    for program in &manifest.programs {
        let program_path =
            validate_corpus_relative_path(corpus_root, &program.path, "program.path")?;
        let source_bytes = std::fs::read(&program_path).map_err(|err| {
            format!(
                "mir representation failed to read {}: {err}",
                program_path.display()
            )
        })?;
        let digest = MirRepresentationDigest {
            algorithm: "sha256".to_string(),
            hex: source_digest_hex(&source_bytes),
        };
        let mir = lower_program_to_mir(&program_path)?;
        programs.push(summarize_mir_program(
            &program.id,
            &program.path,
            digest,
            &mir,
        ));
    }

    let summary = summarize_programs(&programs);
    Ok(MirRepresentationReceipt {
        schema: MIR_REPRESENTATION_SCHEMA.to_string(),
        receipt_id: "mir-representation-semantic-corpus-2026-06-18".to_string(),
        created_at: "2026-06-18".to_string(),
        compiler: "quantac".to_string(),
        language: "quantalang".to_string(),
        source_set: MirRepresentationSourceSet {
            kind: "semantic-corpus".to_string(),
            manifest: "manifest.json".to_string(),
            program_count: manifest.programs.len(),
        },
        ir: MirRepresentationIr {
            name: "MIR".to_string(),
            version: "v0".to_string(),
            lowering_pipeline: "parse -> type-check -> ast-to-mir".to_string(),
        },
        programs,
        summary,
    })
}
```

- [ ] **Step 5: Implement validation**

Add this function and keep diagnostics exact enough for CLI tests:

```rust
pub(crate) fn validate_mir_representation_receipt(
    corpus_root: &Path,
    receipt: &MirRepresentationReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), String> {
    if receipt.schema != MIR_REPRESENTATION_SCHEMA {
        return Err(format!(
            "mir representation receipt has unsupported schema '{}'",
            receipt.schema
        ));
    }
    if receipt.compiler != "quantac" {
        return Err(format!(
            "mir representation compiler mismatch: expected 'quantac', found '{}'",
            receipt.compiler
        ));
    }
    if receipt.language != "quantalang" {
        return Err(format!(
            "mir representation language mismatch: expected 'quantalang', found '{}'",
            receipt.language
        ));
    }
    if receipt.source_set.kind != "semantic-corpus" {
        return Err(format!(
            "mir representation source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        ));
    }
    let manifest_path =
        validate_corpus_relative_path(corpus_root, &receipt.source_set.manifest, "source_set.manifest")?;
    let expected_manifest = corpus_root.join("manifest.json").canonicalize().map_err(|err| {
        format!(
            "mir representation failed to canonicalize expected manifest {}: {err}",
            corpus_root.join("manifest.json").display()
        )
    })?;
    if manifest_path != expected_manifest {
        return Err(format!(
            "mir representation source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        ));
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        return Err(format!(
            "mir representation source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        ));
    }

    let expected = build_mir_representation_receipt(corpus_root, manifest)?;
    compare_receipts(receipt, &expected)
}

pub(crate) fn verify_mir_representation_receipt(
    corpus_root: &Path,
    receipt: &MirRepresentationReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_mir_representation_receipt(corpus_root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}
```

Add `compare_receipts(receipt, expected)` that checks:

```rust
fn compare_receipts(
    receipt: &MirRepresentationReceipt,
    expected: &MirRepresentationReceipt,
) -> Result<(), String>
```

The comparison must report these exact message forms:

```rust
"mir representation program {id} source_digest mismatch"
"mir representation program {id} operations.rvalues drift"
"mir representation program {id} operations.statements drift"
"mir representation program {id} operations.terminators drift"
"mir representation program {id} module counts drift"
"mir representation program {id} symbols drift"
"mir representation program {id} memory_surfaces drift"
"mir representation program {id} control_flow drift"
"mir representation summary drift"
```

Use manifest order by comparing `receipt.programs.len()`, then zipping `receipt.programs` and `expected.programs`. If IDs or paths differ, return:

```rust
format!(
    "mir representation program order drift: expected {} at {}, found {} at {}",
    expected_program.id, expected_program.path, actual_program.id, actual_program.path
)
```

- [ ] **Step 6: Wire `cmd_corpus_verify`**

In `cmd_corpus_verify`, after `let substrate_receipt_path = ...;`, add:

```rust
let mir_receipt_path = receipts_dir.join(MIR_REPRESENTATION_RECEIPT);
let mir_receipt: MirRepresentationReceipt = read_json(&mir_receipt_path)?;
verify_mir_representation_receipt(&corpus_root, &mir_receipt, &manifest)?;
```

In the success output, add this line after `substrate receipt: ok`:

```rust
println!("mir representation receipt: ok");
```

- [ ] **Step 7: Generate and add the checked-in receipt**

Temporarily add this ignored test in `compiler/src/mir_representation.rs`:

```rust
#[test]
#[ignore]
fn print_semantic_corpus_mir_representation_receipt() {
    let corpus_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest has repository parent")
        .join("semantic-corpus");
    let manifest: SemanticCorpusManifest =
        serde_json::from_slice(&std::fs::read(corpus_root.join("manifest.json")).unwrap())
            .unwrap();
    let receipt = build_mir_representation_receipt(&corpus_root, &manifest).unwrap();
    println!("{}", serde_json::to_string_pretty(&receipt).unwrap());
}
```

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml print_semantic_corpus_mir_representation_receipt -- --ignored --nocapture
```

Create `semantic-corpus/receipts/mir-representation-2026-06-18.json` from the JSON printed between the test harness lines, then remove the ignored test before committing. The final committed code must not include this ignored print test.

- [ ] **Step 8: Run tests and confirm GREEN**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml mir_representation --quiet
cargo test --manifest-path compiler\Cargo.toml --test cli mir_representation -- --nocapture
```

Expected: PASS.

- [ ] **Step 9: Commit verifier and receipt**

Run:

```powershell
git add compiler\src\main.rs compiler\src\mir_representation.rs compiler\tests\cli.rs semantic-corpus\receipts\mir-representation-2026-06-18.json
git commit -m "feat: verify mir representation receipts"
```

## Task 4: Substrate Reference and Public Docs

**Files:**
- Modify: `compiler/src/main.rs`
- Modify: `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`
- Modify: `README.md`
- Modify: `STATUS.md`

**Interfaces:**
- Consumes: `MIR_REPRESENTATION_RECEIPT` and existing `validate_substrate_path`.
- Produces: substrate receipt path validation for `representation_surface.representation_receipt`.

- [ ] **Step 1: Add failing substrate reference test**

Add this CLI test after `corpus_verify_rejects_substrate_receipt_path_escape`:

```rust
#[test]
fn corpus_verify_rejects_substrate_representation_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_repr_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String("../outside.json".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against substrate representation receipt path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli substrate_representation -- --nocapture
```

Expected: FAIL because `SubstrateRepresentationSurface` does not yet include or validate `representation_receipt`.

- [ ] **Step 2: Extend substrate DTO and validator**

Change `SubstrateRepresentationSurface` in `compiler/src/main.rs` to:

```rust
#[derive(serde::Deserialize)]
struct SubstrateRepresentationSurface {
    ir: String,
    fallback_policy: String,
    backend_maturity_descriptor: String,
    representation_receipt: String,
}
```

In `validate_substrate_receipt`, after validating `backend_maturity_descriptor`, add:

```rust
let representation_receipt_path = validate_substrate_path(
    corpus_root,
    &receipt.representation_surface.representation_receipt,
    "representation_surface.representation_receipt",
)?;
if representation_receipt_path
    != corpus_root
        .join("receipts")
        .join(MIR_REPRESENTATION_RECEIPT)
        .canonicalize()
        .map_err(|err| {
            format!(
                "substrate representation_surface.representation_receipt failed to canonicalize expected receipt {}: {err}",
                corpus_root.join("receipts").join(MIR_REPRESENTATION_RECEIPT).display()
            )
        })?
{
    return Err(format!(
        "substrate representation_surface.representation_receipt must point at receipts/{}, found {}",
        MIR_REPRESENTATION_RECEIPT,
        receipt.representation_surface.representation_receipt
    ));
}
```

- [ ] **Step 3: Update substrate receipt JSON**

In `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`, change:

```json
  "representation_surface": {
    "ir": "MIR",
    "fallback_policy": "unsupported or partial targets must not claim production maturity",
    "backend_maturity_descriptor": "compiler/src/codegen/backend/STATUS.md"
  },
```

to:

```json
  "representation_surface": {
    "ir": "MIR",
    "fallback_policy": "unsupported or partial targets must not claim production maturity",
    "backend_maturity_descriptor": "compiler/src/codegen/backend/STATUS.md",
    "representation_receipt": "receipts/mir-representation-2026-06-18.json"
  },
```

- [ ] **Step 4: Update README**

In `README.md`, extend the substrate receipt paragraph with:

```markdown
The same verification path now validates a MIR Representation Receipt
(`quantalang-mir-representation-receipt/v0`) that recomputes per-program MIR
module counts, symbols, operation families, memory-surface flags, and
control-flow summaries from the real parse, type-check, and AST-to-MIR lowering
pipeline. This makes the representation claim inspectable without promoting any
experimental backend.
```

- [ ] **Step 5: Update STATUS**

In `STATUS.md`, update the summary paragraph sentence about the substrate receipt to include:

```markdown
Its representation surface is now backed by a checked
`quantalang-mir-representation-receipt/v0` artifact that recomputes per-program
MIR operation families, symbols, memory-surface flags, and control-flow
summaries during `quantac corpus verify`.
```

- [ ] **Step 6: Run tests and confirm GREEN**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli substrate -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli mir_representation -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit substrate and docs**

Run:

```powershell
git add compiler\src\main.rs compiler\tests\cli.rs semantic-corpus\receipts\substrate-semantic-corpus-2026-06-18.json README.md STATUS.md
git commit -m "docs: connect mir representation receipt"
```

## Task 5: Final Verification and Hygiene

**Files:**
- No required source edits unless a verification command exposes a defect.

**Interfaces:**
- Consumes all prior task commits.
- Produces final evidence for handoff or branch completion.

- [ ] **Step 1: Run formatting**

Run:

```powershell
cargo fmt --manifest-path compiler\Cargo.toml -- --check
```

Expected: PASS.

- [ ] **Step 2: Run targeted Rust unit tests**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml mir_representation --quiet
```

Expected: PASS.

- [ ] **Step 3: Run CLI regression slices**

Run:

```powershell
cargo test --manifest-path compiler\Cargo.toml --test cli mir_representation -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli substrate -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler\Cargo.toml --test cli doctor -- --nocapture
```

Expected: PASS.

- [ ] **Step 4: Run corpus verify manually**

Run:

```powershell
cargo run --manifest-path compiler\Cargo.toml -- corpus verify --root semantic-corpus
```

Expected stdout includes:

```text
Semantic Corpus Verify
manifest: 8 program(s)
c receipt: ok
rust receipt: ok
substrate receipt: ok
mir representation receipt: ok
c execution: 8 passed
```

- [ ] **Step 5: Run diff hygiene**

Run:

```powershell
git diff --check
git check-ignore -v .env .env.local
```

Expected: `git diff --check` prints nothing and exits 0. `git check-ignore` shows `.env` and `.env.local` ignored by `.gitignore`.

- [ ] **Step 6: Run added-line high-confidence secret scan**

Run:

```powershell
$base = git merge-base HEAD main
$patterns = @(
  'AKIA[0-9A-Z]{16}',
  'ASIA[0-9A-Z]{16}',
  'AIza[0-9A-Za-z\-_]{35}',
  'sk-[A-Za-z0-9_-]{20,}',
  'ghp_[A-Za-z0-9]{36}',
  'github_pat_[A-Za-z0-9_]{82}',
  'xox[baprs]-[A-Za-z0-9-]{10,}',
  '-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----'
)
$diff = git diff --unified=0 $base..HEAD
$matches = $diff |
  Where-Object { $_ -match '^\+' -and $_ -notmatch '^\+\+\+' } |
  Where-Object {
    $line = $_
    $patterns | Where-Object { $line -match $_ }
  }
if ($matches) {
  $matches
  exit 1
} else {
  'added-line high-confidence secret scan: no matches'
}
```

Expected: `added-line high-confidence secret scan: no matches`.

- [ ] **Step 7: Report final state**

Run:

```powershell
git status --short --branch
git log --oneline --max-count 8
```

Expected: clean working tree on the implementation branch, with the task commits visible.

## Plan Self-Review Notes

- Spec coverage: Tasks cover the checked-in receipt, recomputation from parse/type-check/lower, schema/path/digest/operation drift failures, substrate reference validation, docs, and focused verification gates.
- Scope: The plan does not add a public MIR dump command, public receipt writer, backend promotion, memory-layout ABI proof, symbol graph for packages, or self-hosted compiler claim.
- Type consistency: DTO names, helper names, constant name, file paths, and CLI output string are stable across tasks.
