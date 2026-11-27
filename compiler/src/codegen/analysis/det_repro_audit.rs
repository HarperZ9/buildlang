// ===============================================================================
// BUILDLANG COMPILER - COMPILE-TIME BIT-EXACT DETERMINISM & REPRODUCIBILITY VERIFIER
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Interprocedural MIR bit-exact determinism and reproducibility verifier.
//!
//! Operates strictly at compile time over a lowered [`MirModule`] with zero
//! runtime overhead:
//! 1. Scans every reachable basic block for direct nondeterminism hazards:
//!    - Wall-clock, entropy/RNG, host-environment, and PID calls
//!      (`nondeterministic_call`),
//!    - Pointer-address-to-integer casts (`CastKind::PtrToInt` or pointer-to-int
//!      `CastKind::Transmute`) that leak ASLR/allocator addresses into hashes or
//!      ordering (`pointer_to_int_cast`),
//!    - Direct floating-point equality/inequality (`f32`/`f64` `==` / `!=`)
//!      vulnerable to IEEE-754 `NaN != NaN`, `-0.0 == +0.0`, and cross-target
//!      rounding drift (`direct_float_equality`).
//! 2. Propagates hazards backward across the interprocedural call graph via
//!    deterministic reverse-BFS to compute canonical shortest call-chain
//!    witnesses for every affected function.
//! 3. Emits a canonical deterministic JSON manifest (`buildlang.det-manifest/v1`)
//!    sealed with `manifest_sha256` and fails closed on any violation.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::codegen::analysis::cfg::reachable_blocks;
use crate::codegen::ir::{
    BinOp, CastKind, FloatSize, MirConst, MirFunction, MirModule, MirRValue, MirStmtKind,
    MirTerminator, MirType, MirValue,
};

/// Canonical schema identifier for `--det-manifest-json`.
pub const DET_MANIFEST_SCHEMA: &str = "buildlang.det-manifest/v1";

/// Compile-time determinism hazard category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetViolationKind {
    /// Call to wall-clock, RNG/entropy, host environment, or PID source.
    NondeterministicCall,
    /// Pointer-address-to-integer cast (`PtrToInt` or pointer-to-int transmute).
    PointerToIntCast,
    /// Direct `f32`/`f64` equality or inequality (`==` / `!=`) NaN/ULP hazard.
    DirectFloatEquality,
}

/// Fail-closed determinism violation with exact interprocedural call-chain trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetViolation {
    /// Affected caller or defining function.
    pub function: String,
    /// Nondeterminism hazard category.
    pub kind: DetViolationKind,
    /// Ordered shortest call chain from `function` to triggering op or symbol.
    pub call_chain: Vec<String>,
    /// Terminal trigger token (e.g. `call:build_clock_ms`, `op:ptr_to_int`, `op:float_eq(f64)`).
    pub trigger: String,
    /// Human-readable fail-closed diagnostic message.
    pub message: String,
}

/// Per-function static determinism summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionDetRecord {
    /// Function name.
    pub name: String,
    /// True if zero direct or transitive determinism hazards reach this function.
    pub deterministic: bool,
    /// Sorted direct hazard kinds inside this function.
    pub direct_hazards: Vec<DetViolationKind>,
    /// Sorted transitive hazard kinds reachable from this function.
    pub transitive_hazards: Vec<DetViolationKind>,
    /// Sorted direct in-module callees.
    pub callees: Vec<String>,
}

/// Canonical module-level bit-exact determinism and reproducibility manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetManifest {
    /// Schema tag (`buildlang.det-manifest/v1`).
    pub schema: String,
    /// Audited module name or source path.
    pub module: String,
    /// True when zero direct or transitive determinism hazards exist in the module.
    pub bit_exact_deterministic: bool,
    /// Total functions audited.
    pub function_count: usize,
    /// Number of functions proven free of all determinism hazards.
    pub deterministic_function_count: usize,
    /// Total fail-closed violations across all functions.
    pub violation_count: usize,
    /// True iff `violation_count == 0`.
    pub passed: bool,
    /// Sorted per-function determinism records.
    pub functions: Vec<FunctionDetRecord>,
    /// Sorted fail-closed determinism violations.
    pub violations: Vec<DetViolation>,
    /// Lowercase hex SHA-256 over the canonical JSON payload (excluding this field).
    pub manifest_sha256: String,
}

#[derive(Serialize)]
struct DetManifestHashPayload<'a> {
    schema: &'a str,
    module: &'a str,
    bit_exact_deterministic: bool,
    function_count: usize,
    deterministic_function_count: usize,
    violation_count: usize,
    passed: bool,
    functions: &'a [FunctionDetRecord],
    violations: &'a [DetViolation],
}

/// Recompute the canonical SHA-256 digest of a [`DetManifest`].
pub fn compute_det_manifest_sha256(manifest: &DetManifest) -> Result<String, String> {
    let payload = DetManifestHashPayload {
        schema: &manifest.schema,
        module: &manifest.module,
        bit_exact_deterministic: manifest.bit_exact_deterministic,
        function_count: manifest.function_count,
        deterministic_function_count: manifest.deterministic_function_count,
        violation_count: manifest.violation_count,
        passed: manifest.passed,
        functions: &manifest.functions,
        violations: &manifest.violations,
    };
    let bytes =
        serde_json::to_vec(&payload).map_err(|e| format!("det manifest serialize error: {e}"))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Classify a runtime or C library symbol as a nondeterminism source
/// (wall-clock, entropy/RNG, host environment, or process/thread ID).
///
/// Returns `Some(category)` when the symbol is nondeterministic, or `None` otherwise.
pub fn classify_nondeterministic_symbol(name: &str) -> Option<&'static str> {
    // 1. Wall-clock / timer / sleep
    if matches!(
        name,
        "time"
            | "clock"
            | "gettimeofday"
            | "clock_gettime"
            | "nanosleep"
            | "usleep"
            | "sleep"
            | "clock_ms"
            | "clock_ns"
            | "time_unix"
            | "build_clock_ms"
            | "build_clock_ns"
            | "build_time_unix"
    ) || name.starts_with("build_clock_")
        || name.starts_with("build_time_")
        || name.starts_with("build_sleep_")
    {
        return Some("clock");
    }

    // 2. Entropy / nondeterministic RNG
    if matches!(
        name,
        "rand"
            | "srand"
            | "random"
            | "srandom"
            | "drand48"
            | "lrand48"
            | "arc4random"
            | "arc4random_buf"
            | "getrandom"
            | "getentropy"
            | "random_f64"
            | "random_u64"
            | "build_random_f64"
            | "build_random_u64"
    ) || name.starts_with("build_rand")
        || name.starts_with("build_random")
        || name.starts_with("build_entropy")
    {
        return Some("random");
    }

    // 3. Host environment / process & thread IDs
    if matches!(
        name,
        "getenv"
            | "setenv"
            | "unsetenv"
            | "getpid"
            | "getppid"
            | "gettid"
            | "getuid"
            | "geteuid"
            | "gethostname"
            | "build_getenv"
            | "build_getpid"
    ) || name.starts_with("build_env_")
    {
        return Some("env_pid");
    }

    None
}

fn value_mir_type<'a>(val: &'a MirValue, func: &'a MirFunction) -> Option<&'a MirType> {
    match val {
        MirValue::Local(id) => func.locals.iter().find(|l| l.id == *id).map(|l| &l.ty),
        MirValue::Const(c) => match c {
            MirConst::Int(_, ty)
            | MirConst::Uint(_, ty)
            | MirConst::Float(_, ty)
            | MirConst::Null(ty)
            | MirConst::Zeroed(ty)
            | MirConst::Undef(ty)
            | MirConst::Array(ty, _) => Some(ty),
            _ => None,
        },
        _ => None,
    }
}

fn float_label(ty: &MirType) -> Option<&'static str> {
    match ty {
        MirType::Float(FloatSize::F32) => Some("f32"),
        MirType::Float(FloatSize::F64) => Some("f64"),
        _ => None,
    }
}

fn scan_rvalue_hazards(
    rv: &MirRValue,
    func: &MirFunction,
    direct: &mut BTreeMap<DetViolationKind, String>,
) {
    match rv {
        MirRValue::Cast { kind, value, ty } => match kind {
            CastKind::PtrToInt => {
                direct
                    .entry(DetViolationKind::PointerToIntCast)
                    .or_insert_with(|| "op:ptr_to_int".to_string());
            }
            CastKind::Transmute => {
                let src_is_ptr = matches!(value_mir_type(value, func), Some(MirType::Ptr(_)));
                let dst_is_int = matches!(ty, MirType::Int(_, _));
                if src_is_ptr && dst_is_int {
                    direct
                        .entry(DetViolationKind::PointerToIntCast)
                        .or_insert_with(|| "op:ptr_to_int_transmute".to_string());
                }
            }
            _ => {}
        },
        MirRValue::BinaryOp {
            op: op @ (BinOp::Eq | BinOp::Ne),
            left,
            right,
        } => {
            let fty = value_mir_type(left, func)
                .and_then(float_label)
                .or_else(|| value_mir_type(right, func).and_then(float_label));
            if let Some(fl) = fty {
                let op_str = if matches!(op, BinOp::Eq) {
                    "float_eq"
                } else {
                    "float_ne"
                };
                direct
                    .entry(DetViolationKind::DirectFloatEquality)
                    .or_insert_with(|| format!("op:{op_str}({fl})"));
            }
        }
        _ => {}
    }
}

struct FunctionDetScan {
    direct: BTreeMap<DetViolationKind, String>,
    callees: BTreeSet<String>,
}

fn scan_function(func: &MirFunction, all_fn_names: &BTreeSet<String>) -> FunctionDetScan {
    let mut direct = BTreeMap::new();
    let mut callees = BTreeSet::new();

    if func.blocks.is_none() {
        if let Some(cat) = classify_nondeterministic_symbol(func.name.as_ref()) {
            direct.insert(
                DetViolationKind::NondeterministicCall,
                format!("extern:{cat}:{}", func.name),
            );
        }
        return FunctionDetScan { direct, callees };
    }

    if let Some(blocks) = &func.blocks {
        let reach = reachable_blocks(blocks);
        for (bi, block) in blocks.iter().enumerate() {
            if !reach[bi] {
                continue;
            }
            for stmt in &block.stmts {
                let rvalue_opt = match &stmt.kind {
                    MirStmtKind::Assign { value, .. }
                    | MirStmtKind::DerefAssign { value, .. }
                    | MirStmtKind::FieldDerefAssign { value, .. }
                    | MirStmtKind::FieldAssign { value, .. }
                    | MirStmtKind::IndexStore { value, .. }
                    | MirStmtKind::GlobalStore { value, .. } => Some(value),
                    MirStmtKind::WorkgroupBarrier
                    | MirStmtKind::StorageLive(_)
                    | MirStmtKind::StorageDead(_)
                    | MirStmtKind::Nop => None,
                };
                if let Some(rv) = rvalue_opt {
                    scan_rvalue_hazards(rv, func, &mut direct);
                }
            }

            if let Some(MirTerminator::Call { func: callee, .. }) = &block.terminator {
                if let MirValue::Function(name) | MirValue::Global(name) = callee {
                    let cname = name.as_ref();
                    if let Some(cat) = classify_nondeterministic_symbol(cname) {
                        direct
                            .entry(DetViolationKind::NondeterministicCall)
                            .or_insert_with(|| format!("call:{cat}:{cname}"));
                    } else if all_fn_names.contains(cname) {
                        callees.insert(cname.to_string());
                    }
                }
            }
        }
    }

    FunctionDetScan { direct, callees }
}

/// Run the compile-time bit-exact determinism and reproducibility verifier over
/// a lowered [`MirModule`].
pub fn audit_mir_determinism(
    module: &MirModule,
    module_label: &str,
) -> Result<DetManifest, String> {
    let mut fn_map: BTreeMap<String, &MirFunction> = BTreeMap::new();
    for f in &module.functions {
        fn_map.insert(f.name.to_string(), f);
    }
    let all_fn_names: BTreeSet<String> = fn_map.keys().cloned().collect();

    // 1. Direct per-function scans + reverse call graph.
    let mut scans: BTreeMap<String, FunctionDetScan> = BTreeMap::new();
    let mut rev_callers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (fname, func) in &fn_map {
        let scan = scan_function(func, &all_fn_names);
        for callee in &scan.callees {
            rev_callers
                .entry(callee.clone())
                .or_default()
                .insert(fname.clone());
        }
        scans.insert(fname.clone(), scan);
    }

    // 2. Reverse-BFS per hazard kind to compute canonical shortest call-chain witnesses.
    let all_kinds = [
        DetViolationKind::NondeterministicCall,
        DetViolationKind::PointerToIntCast,
        DetViolationKind::DirectFloatEquality,
    ];
    let mut witnesses: BTreeMap<String, BTreeMap<DetViolationKind, Vec<String>>> = BTreeMap::new();

    for kind in all_kinds {
        let mut queue: VecDeque<String> = VecDeque::new();
        for (fname, scan) in &scans {
            if let Some(trigger) = scan.direct.get(&kind) {
                witnesses
                    .entry(fname.clone())
                    .or_default()
                    .insert(kind, vec![fname.clone(), trigger.clone()]);
                queue.push_back(fname.clone());
            }
        }
        while let Some(callee) = queue.pop_front() {
            let callee_chain = witnesses[&callee][&kind].clone();
            if let Some(callers) = rev_callers.get(&callee) {
                for caller in callers {
                    let already = witnesses.get(caller).and_then(|m| m.get(&kind)).is_some();
                    if !already {
                        let mut chain = Vec::with_capacity(callee_chain.len() + 1);
                        chain.push(caller.clone());
                        chain.extend(callee_chain.iter().cloned());
                        witnesses
                            .entry(caller.clone())
                            .or_default()
                            .insert(kind, chain);
                        queue.push_back(caller.clone());
                    }
                }
            }
        }
    }

    // 3. Assemble sorted per-function records and violations.
    let mut records = Vec::with_capacity(fn_map.len());
    let mut violations = Vec::new();
    let mut det_fn_count = 0usize;

    for (fname, scan) in &scans {
        let direct_hazards: Vec<DetViolationKind> = scan.direct.keys().copied().collect();
        let fn_witnesses = witnesses.get(fname);
        let transitive_hazards: Vec<DetViolationKind> = fn_witnesses
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        let deterministic = transitive_hazards.is_empty();
        if deterministic {
            det_fn_count += 1;
        }
        if let Some(wmap) = fn_witnesses {
            for (&kind, chain) in wmap {
                let trigger = chain.last().cloned().unwrap_or_default();
                let kind_desc = match kind {
                    DetViolationKind::NondeterministicCall => "nondeterministic clock/rng/env call",
                    DetViolationKind::PointerToIntCast => "pointer-to-integer address cast",
                    DetViolationKind::DirectFloatEquality => {
                        "direct floating-point equality/inequality"
                    }
                };
                let trace = chain.join(" -> ");
                violations.push(DetViolation {
                    function: fname.clone(),
                    kind,
                    call_chain: chain.clone(),
                    trigger,
                    message: format!("{kind_desc} reachable from `{fname}` via {trace}"),
                });
            }
        }
        records.push(FunctionDetRecord {
            name: fname.clone(),
            deterministic,
            direct_hazards,
            transitive_hazards,
            callees: scan.callees.iter().cloned().collect(),
        });
    }

    let function_count = records.len();
    let violation_count = violations.len();
    let passed = violation_count == 0;

    let mut manifest = DetManifest {
        schema: DET_MANIFEST_SCHEMA.to_string(),
        module: module_label.to_string(),
        bit_exact_deterministic: passed,
        function_count,
        deterministic_function_count: det_fn_count,
        violation_count,
        passed,
        functions: records,
        violations,
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = compute_det_manifest_sha256(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{BlockId, LocalId, MirBlock, MirFnSig, MirLocal, MirStmt};
    use std::sync::Arc;

    #[test]
    fn clean_integer_and_ordered_float_kernel_passes_strict_determinism() {
        let mut m = MirModule::new("clean_det");
        let mut f = MirFunction::new(
            "kernel",
            MirFnSig::new(vec![MirType::f64(), MirType::f64()], MirType::Bool),
        );
        let mut p0 = MirLocal::named(LocalId(0), "a", MirType::f64());
        p0.is_param = true;
        let mut p1 = MirLocal::named(LocalId(1), "b", MirType::f64());
        p1.is_param = true;
        f.add_local(p0);
        f.add_local(p1);
        f.add_local(MirLocal::new(LocalId(2), MirType::f64()));
        f.add_local(MirLocal::new(LocalId(3), MirType::Bool));

        let mut bb = MirBlock::new(BlockId(0));
        bb.push_stmt(MirStmt::assign(
            LocalId(2),
            MirRValue::BinaryOp {
                op: BinOp::Sub,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Local(LocalId(1)),
            },
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(3),
            MirRValue::BinaryOp {
                op: BinOp::Le,
                left: MirValue::Local(LocalId(2)),
                right: MirValue::Const(MirConst::Float(1e-9, MirType::f64())),
            },
        ));
        bb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(3)))));
        f.add_block(bb);
        m.add_function(f);

        let manifest = audit_mir_determinism(&m, "clean_det.bld").unwrap();
        assert!(manifest.passed);
        assert!(manifest.bit_exact_deterministic);
        assert_eq!(manifest.violation_count, 0);
        assert_eq!(
            compute_det_manifest_sha256(&manifest).unwrap(),
            manifest.manifest_sha256
        );
    }

    #[test]
    fn two_hop_clock_call_ptr_to_int_and_float_eq_all_fail_closed() {
        let mut m = MirModule::new("hazards");

        // leaf_clock calls build_clock_ms
        let mut leaf = MirFunction::new("leaf_clock", MirFnSig::new(vec![], MirType::i64()));
        let mut lbb0 = MirBlock::new(BlockId(0));
        lbb0.set_terminator(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_clock_ms")),
            args: vec![],
            dest: None,
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut lbb1 = MirBlock::new(BlockId(1));
        lbb1.set_terminator(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
            0,
            MirType::i64(),
        )))));
        leaf.add_block(lbb0);
        leaf.add_block(lbb1);
        m.add_function(leaf);

        // main -> hop_a -> leaf_clock
        let mut hop_a = MirFunction::new("hop_a", MirFnSig::new(vec![], MirType::i64()));
        let mut hbb0 = MirBlock::new(BlockId(0));
        hbb0.set_terminator(MirTerminator::Call {
            func: MirValue::Function(Arc::from("leaf_clock")),
            args: vec![],
            dest: None,
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut hbb1 = MirBlock::new(BlockId(1));
        hbb1.set_terminator(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
            0,
            MirType::i64(),
        )))));
        hop_a.add_block(hbb0);
        hop_a.add_block(hbb1);
        m.add_function(hop_a);

        let mut main_fn = MirFunction::new("main", MirFnSig::new(vec![], MirType::i32()));
        main_fn.add_local(MirLocal::new(
            LocalId(0),
            MirType::Ptr(Box::new(MirType::i64())),
        ));
        main_fn.add_local(MirLocal::new(LocalId(1), MirType::usize()));
        main_fn.add_local(MirLocal::new(LocalId(2), MirType::f64()));
        main_fn.add_local(MirLocal::new(LocalId(3), MirType::Bool));
        let mut mbb0 = MirBlock::new(BlockId(0));
        mbb0.push_stmt(MirStmt::assign(
            LocalId(1),
            MirRValue::Cast {
                kind: CastKind::PtrToInt,
                value: MirValue::Local(LocalId(0)),
                ty: MirType::usize(),
            },
        ));
        mbb0.push_stmt(MirStmt::assign(
            LocalId(3),
            MirRValue::BinaryOp {
                op: BinOp::Eq,
                left: MirValue::Local(LocalId(2)),
                right: MirValue::Const(MirConst::Float(1.0, MirType::f64())),
            },
        ));
        mbb0.set_terminator(MirTerminator::Call {
            func: MirValue::Function(Arc::from("hop_a")),
            args: vec![],
            dest: None,
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut mbb1 = MirBlock::new(BlockId(1));
        mbb1.set_terminator(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
            0,
            MirType::i32(),
        )))));
        main_fn.add_block(mbb0);
        main_fn.add_block(mbb1);
        m.add_function(main_fn);

        let manifest = audit_mir_determinism(&m, "hazards.bld").unwrap();
        assert!(!manifest.passed);
        assert!(!manifest.bit_exact_deterministic);

        let clock_v = manifest
            .violations
            .iter()
            .find(|v| v.function == "main" && v.kind == DetViolationKind::NondeterministicCall)
            .expect("main must report 2-hop clock call");
        assert_eq!(
            clock_v.call_chain,
            vec!["main", "hop_a", "leaf_clock", "call:clock:build_clock_ms"]
        );
        assert!(manifest
            .violations
            .iter()
            .any(|v| v.function == "main" && v.kind == DetViolationKind::PointerToIntCast));
        assert!(manifest
            .violations
            .iter()
            .any(|v| v.function == "main" && v.kind == DetViolationKind::DirectFloatEquality));
    }

    #[test]
    fn symbol_classifier_covers_clock_random_env_and_allows_pure() {
        assert_eq!(
            classify_nondeterministic_symbol("build_clock_ms"),
            Some("clock")
        );
        assert_eq!(
            classify_nondeterministic_symbol("build_random_f64"),
            Some("random")
        );
        assert_eq!(
            classify_nondeterministic_symbol("build_getenv"),
            Some("env_pid")
        );
        assert_eq!(classify_nondeterministic_symbol("sha256_bytes"), None);
    }
}
