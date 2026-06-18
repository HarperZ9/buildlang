#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use quantalang::codegen::{
    lower::MirLowerer, AggregateKind, BinOp, CastKind, MirModule, MirPlace, MirRValue, MirStmtKind,
    MirTerminator, PlaceProjection, UnaryOp,
};
use quantalang::lexer::{Lexer, SourceFile};
use quantalang::parser::Parser;
use quantalang::types::{TypeChecker, TypeContext};

use super::{
    preprocess_includes, resolve_imports, resolve_modules, source_digest_hex,
    SemanticCorpusManifest,
};

pub(crate) const MIR_REPRESENTATION_RECEIPT: &str = "mir-representation-2026-06-18.json";

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
        globals: sorted(
            module
                .globals
                .iter()
                .map(|global| global.name.to_string())
                .collect(),
        ),
        externals: sorted(
            module
                .externals
                .iter()
                .map(|external| external.name.to_string())
                .collect(),
        ),
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
                collect_loop_edges(terminator, block.id.0, &mut control_flow);
                collect_terminator(
                    terminator,
                    &mut sets,
                    &mut memory_surfaces,
                    &mut control_flow,
                );
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

fn collect_stmt(
    stmt: &MirStmtKind,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
) {
    sets.statements.insert(statement_name(stmt).to_string());
    match stmt {
        MirStmtKind::Assign { value, .. } => collect_rvalue(value, sets, memory),
        MirStmtKind::DerefAssign { value, .. } => {
            memory.deref_writes = true;
            collect_rvalue(value, sets, memory);
        }
        MirStmtKind::FieldDerefAssign { value, .. } => {
            memory.deref_writes = true;
            memory.field_writes = true;
            collect_rvalue(value, sets, memory);
        }
        MirStmtKind::FieldAssign { value, .. } => {
            memory.field_writes = true;
            collect_rvalue(value, sets, memory);
        }
        MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop => {}
    }
}

fn collect_rvalue(
    rvalue: &MirRValue,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
) {
    sets.rvalues.insert(rvalue_name(rvalue).to_string());
    match rvalue {
        MirRValue::Use(_) => {}
        MirRValue::BinaryOp { op, .. } => {
            sets.binary_ops.insert(bin_op_name(*op).to_string());
        }
        MirRValue::UnaryOp { op, .. } => {
            sets.unary_ops.insert(unary_op_name(*op).to_string());
        }
        MirRValue::Ref { is_mut, place } | MirRValue::AddressOf { is_mut, place } => {
            memory.references = true;
            memory.mutable_references |= *is_mut;
            collect_place(place, sets, memory);
        }
        MirRValue::Cast { kind, .. } => {
            sets.casts.insert(cast_kind_name(*kind).to_string());
        }
        MirRValue::Aggregate { kind, .. } => {
            memory.aggregate_values = true;
            sets.aggregate_kinds
                .insert(aggregate_kind_name(kind).to_string());
        }
        MirRValue::Repeat { .. } => {}
        MirRValue::Discriminant(place) | MirRValue::Len(place) => {
            collect_place(place, sets, memory);
        }
        MirRValue::NullaryOp(_, _) => {}
        MirRValue::FieldAccess { .. } | MirRValue::VariantField { .. } => {
            memory.field_reads = true;
        }
        MirRValue::IndexAccess { .. } => {
            memory.index_reads = true;
        }
        MirRValue::Deref { .. } => {
            memory.deref_reads = true;
        }
        MirRValue::TextureSample { .. } => {}
    }
}

fn collect_place(
    place: &MirPlace,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
) {
    for projection in &place.projections {
        sets.place_projections
            .insert(projection_name(projection).to_string());
        match projection {
            PlaceProjection::Deref => memory.deref_reads = true,
            PlaceProjection::Field(_, _) => memory.field_reads = true,
            PlaceProjection::Index(_)
            | PlaceProjection::ConstantIndex { .. }
            | PlaceProjection::Subslice { .. } => memory.index_reads = true,
            PlaceProjection::Downcast(_) => {}
        }
    }
}

fn collect_terminator(
    terminator: &MirTerminator,
    sets: &mut InventorySets,
    memory: &mut MirRepresentationMemorySurfaces,
    control_flow: &mut MirRepresentationControlFlow,
) {
    sets.terminators
        .insert(terminator_name(terminator).to_string());
    match terminator {
        MirTerminator::Goto(_) => {}
        MirTerminator::If { .. } => {
            control_flow.branching = true;
        }
        MirTerminator::Switch { .. } => {
            control_flow.switching = true;
        }
        MirTerminator::Call { .. } => {
            control_flow.calls = true;
        }
        MirTerminator::Return(_) => {}
        MirTerminator::Unreachable => {
            control_flow.unreachable = true;
        }
        MirTerminator::Drop { place, .. } => {
            collect_place(place, sets, memory);
        }
        MirTerminator::Assert { .. } => {
            control_flow.branching = true;
        }
        MirTerminator::Resume | MirTerminator::Abort => {}
    }
}

fn collect_loop_edges(
    terminator: &MirTerminator,
    current_block: u32,
    control_flow: &mut MirRepresentationControlFlow,
) {
    let mut note_target = |target: u32| {
        if target <= current_block {
            control_flow.loops = true;
        }
    };

    match terminator {
        MirTerminator::Goto(target) => note_target(target.0),
        MirTerminator::If {
            then_block,
            else_block,
            ..
        } => {
            note_target(then_block.0);
            note_target(else_block.0);
        }
        MirTerminator::Switch {
            targets, default, ..
        } => {
            for (_, target) in targets {
                note_target(target.0);
            }
            note_target(default.0);
        }
        MirTerminator::Call { target, unwind, .. } => {
            if let Some(target) = target {
                note_target(target.0);
            }
            if let Some(unwind) = unwind {
                note_target(unwind.0);
            }
        }
        MirTerminator::Drop { target, unwind, .. }
        | MirTerminator::Assert { target, unwind, .. } => {
            note_target(target.0);
            if let Some(unwind) = unwind {
                note_target(unwind.0);
            }
        }
        MirTerminator::Return(_)
        | MirTerminator::Unreachable
        | MirTerminator::Resume
        | MirTerminator::Abort => {}
    }
}

fn statement_name(stmt: &MirStmtKind) -> &'static str {
    match stmt {
        MirStmtKind::Assign { .. } => "Assign",
        MirStmtKind::DerefAssign { .. } => "DerefAssign",
        MirStmtKind::FieldDerefAssign { .. } => "FieldDerefAssign",
        MirStmtKind::FieldAssign { .. } => "FieldAssign",
        MirStmtKind::StorageLive(_) => "StorageLive",
        MirStmtKind::StorageDead(_) => "StorageDead",
        MirStmtKind::Nop => "Nop",
    }
}

fn rvalue_name(rvalue: &MirRValue) -> &'static str {
    match rvalue {
        MirRValue::Use(_) => "Use",
        MirRValue::BinaryOp { .. } => "BinaryOp",
        MirRValue::UnaryOp { .. } => "UnaryOp",
        MirRValue::Ref { .. } => "Ref",
        MirRValue::AddressOf { .. } => "AddressOf",
        MirRValue::Cast { .. } => "Cast",
        MirRValue::Aggregate { .. } => "Aggregate",
        MirRValue::Repeat { .. } => "Repeat",
        MirRValue::Discriminant(_) => "Discriminant",
        MirRValue::Len(_) => "Len",
        MirRValue::NullaryOp(_, _) => "NullaryOp",
        MirRValue::FieldAccess { .. } => "FieldAccess",
        MirRValue::VariantField { .. } => "VariantField",
        MirRValue::IndexAccess { .. } => "IndexAccess",
        MirRValue::Deref { .. } => "Deref",
        MirRValue::TextureSample { .. } => "TextureSample",
    }
}

fn terminator_name(terminator: &MirTerminator) -> &'static str {
    match terminator {
        MirTerminator::Goto(_) => "Goto",
        MirTerminator::If { .. } => "If",
        MirTerminator::Switch { .. } => "Switch",
        MirTerminator::Call { .. } => "Call",
        MirTerminator::Return(_) => "Return",
        MirTerminator::Unreachable => "Unreachable",
        MirTerminator::Drop { .. } => "Drop",
        MirTerminator::Assert { .. } => "Assert",
        MirTerminator::Resume => "Resume",
        MirTerminator::Abort => "Abort",
    }
}

fn bin_op_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "Add",
        BinOp::Sub => "Sub",
        BinOp::Mul => "Mul",
        BinOp::Div => "Div",
        BinOp::Rem => "Rem",
        BinOp::Pow => "Pow",
        BinOp::BitAnd => "BitAnd",
        BinOp::BitOr => "BitOr",
        BinOp::BitXor => "BitXor",
        BinOp::Shl => "Shl",
        BinOp::Shr => "Shr",
        BinOp::Eq => "Eq",
        BinOp::Ne => "Ne",
        BinOp::Lt => "Lt",
        BinOp::Le => "Le",
        BinOp::Gt => "Gt",
        BinOp::Ge => "Ge",
        BinOp::AddChecked => "AddChecked",
        BinOp::SubChecked => "SubChecked",
        BinOp::MulChecked => "MulChecked",
        BinOp::AddWrapping => "AddWrapping",
        BinOp::SubWrapping => "SubWrapping",
        BinOp::MulWrapping => "MulWrapping",
        BinOp::AddSaturating => "AddSaturating",
        BinOp::SubSaturating => "SubSaturating",
    }
}

fn unary_op_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "Not",
        UnaryOp::Neg => "Neg",
    }
}

fn cast_kind_name(kind: CastKind) -> &'static str {
    match kind {
        CastKind::IntToInt => "IntToInt",
        CastKind::FloatToFloat => "FloatToFloat",
        CastKind::IntToFloat => "IntToFloat",
        CastKind::FloatToInt => "FloatToInt",
        CastKind::PtrToInt => "PtrToInt",
        CastKind::IntToPtr => "IntToPtr",
        CastKind::PtrToPtr => "PtrToPtr",
        CastKind::FnToPtr => "FnToPtr",
        CastKind::Transmute => "Transmute",
    }
}

fn aggregate_kind_name(kind: &AggregateKind) -> &'static str {
    match kind {
        AggregateKind::Array(_) => "Array",
        AggregateKind::Tuple => "Tuple",
        AggregateKind::Struct(_) => "Struct",
        AggregateKind::Variant(_, _, _) => "Variant",
        AggregateKind::Closure(_) => "Closure",
    }
}

fn projection_name(projection: &PlaceProjection) -> &'static str {
    match projection {
        PlaceProjection::Deref => "Deref",
        PlaceProjection::Field(_, _) => "Field",
        PlaceProjection::Index(_) => "Index",
        PlaceProjection::ConstantIndex { .. } => "ConstantIndex",
        PlaceProjection::Subslice { .. } => "Subslice",
        PlaceProjection::Downcast(_) => "Downcast",
    }
}

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
    for program in &receipt.programs {
        validate_corpus_relative_path(corpus_root, &program.path, "program.path")?;
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
        || relative_path.has_root()
        || relative_path
            .components()
            .any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
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
    let source = resolve_imports(&source, program_path).map_err(|_| {
        format!(
            "mir representation failed to resolve imports for {}",
            program_path.display()
        )
    })?;
    let base_dir = program_path.parent().unwrap_or_else(|| Path::new("."));
    let source = preprocess_includes(&source, base_dir).map_err(|_| {
        format!(
            "mir representation failed to preprocess includes for {}",
            program_path.display()
        )
    })?;
    let source_file = SourceFile::new(program_path.to_string_lossy(), source);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|err| {
        format!(
            "mir representation lexer error in {}: {err}",
            program_path.display()
        )
    })?;
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

    MirLowerer::with_source(&ctx, Arc::from(source_file.source()))
        .lower_module(&ast)
        .map_err(|err| {
            format!(
                "mir representation code generation error in {}: {err}",
                program_path.display()
            )
        })
}

fn compare_receipts(
    receipt: &MirRepresentationReceipt,
    expected: &MirRepresentationReceipt,
) -> Result<(), String> {
    if receipt.receipt_id != expected.receipt_id {
        return Err(format!(
            "mir representation receipt_id mismatch: expected '{}', found '{}'",
            expected.receipt_id, receipt.receipt_id
        ));
    }
    if receipt.created_at != expected.created_at {
        return Err(format!(
            "mir representation created_at mismatch: expected '{}', found '{}'",
            expected.created_at, receipt.created_at
        ));
    }
    if receipt.ir != expected.ir {
        return Err("mir representation ir drift".to_string());
    }
    if receipt.programs.len() != expected.programs.len() {
        return Err(format!(
            "mir representation program count drift: expected {}, found {}",
            expected.programs.len(),
            receipt.programs.len()
        ));
    }

    for (actual_program, expected_program) in receipt.programs.iter().zip(&expected.programs) {
        if actual_program.id != expected_program.id || actual_program.path != expected_program.path
        {
            return Err(format!(
                "mir representation program order drift: expected {} at {}, found {} at {}",
                expected_program.id, expected_program.path, actual_program.id, actual_program.path
            ));
        }
        if actual_program.source_digest != expected_program.source_digest {
            return Err(format!(
                "mir representation program {} source_digest mismatch",
                actual_program.id
            ));
        }
        if actual_program.operations.statements != expected_program.operations.statements {
            return Err(format!(
                "mir representation program {} operations.statements drift",
                actual_program.id
            ));
        }
        if actual_program.operations.rvalues != expected_program.operations.rvalues {
            return Err(format!(
                "mir representation program {} operations.rvalues drift",
                actual_program.id
            ));
        }
        if actual_program.operations.terminators != expected_program.operations.terminators {
            return Err(format!(
                "mir representation program {} operations.terminators drift",
                actual_program.id
            ));
        }
        if actual_program.operations.binary_ops != expected_program.operations.binary_ops {
            return Err(format!(
                "mir representation program {} operations.binary_ops drift",
                actual_program.id
            ));
        }
        if actual_program.operations.unary_ops != expected_program.operations.unary_ops {
            return Err(format!(
                "mir representation program {} operations.unary_ops drift",
                actual_program.id
            ));
        }
        if actual_program.operations.casts != expected_program.operations.casts {
            return Err(format!(
                "mir representation program {} operations.casts drift",
                actual_program.id
            ));
        }
        if actual_program.operations.aggregate_kinds != expected_program.operations.aggregate_kinds
        {
            return Err(format!(
                "mir representation program {} operations.aggregate_kinds drift",
                actual_program.id
            ));
        }
        if actual_program.operations.place_projections
            != expected_program.operations.place_projections
        {
            return Err(format!(
                "mir representation program {} operations.place_projections drift",
                actual_program.id
            ));
        }
        if actual_program.module != expected_program.module {
            return Err(format!(
                "mir representation program {} module counts drift",
                actual_program.id
            ));
        }
        if actual_program.symbols != expected_program.symbols {
            return Err(format!(
                "mir representation program {} symbols drift",
                actual_program.id
            ));
        }
        if actual_program.memory_surfaces != expected_program.memory_surfaces {
            return Err(format!(
                "mir representation program {} memory_surfaces drift",
                actual_program.id
            ));
        }
        if actual_program.control_flow != expected_program.control_flow {
            return Err(format!(
                "mir representation program {} control_flow drift",
                actual_program.id
            ));
        }
    }

    if receipt.summary != expected.summary {
        return Err("mir representation summary drift".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use quantalang::codegen::{
        AggregateKind, BlockId, LocalId, MirBlock, MirConst, MirFnSig, MirFunction, MirLocal,
        MirStmt, MirType, MirTypeDef, MirValue, TypeDefKind,
    };

    #[test]
    fn mir_representation_builds_receipt_for_semantic_corpus() {
        let corpus_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
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

    #[test]
    fn validate_corpus_relative_path_rejects_root_qualified_paths() {
        let corpus_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("compiler manifest has repository parent")
            .join("semantic-corpus");
        let rooted = if cfg!(windows) {
            "\\manifest.json"
        } else {
            "/manifest.json"
        };

        let err = validate_corpus_relative_path(&corpus_root, rooted, "program.path")
            .expect_err("root-qualified path should be rejected");

        assert!(err.contains("must stay within corpus root"), "{err}");
    }

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
        func.add_local(MirLocal::new(
            LocalId(0),
            MirType::Struct(Arc::from("Point")),
        ));
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

    fn quantalang_type_def_point() -> MirTypeDef {
        MirTypeDef {
            name: Arc::from("Point"),
            kind: TypeDefKind::Struct {
                fields: vec![(Some(Arc::from("x")), MirType::i32())],
                packed: false,
            },
        }
    }
}
