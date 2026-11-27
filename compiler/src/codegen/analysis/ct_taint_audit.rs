// ===============================================================================
// BUILDLANG COMPILER - COMPILE-TIME CONSTANT-TIME CRYPTOGRAPHY TAINT VERIFIER
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Interprocedural MIR constant-time cryptography and side-channel taint verifier.
//!
//! Operates strictly at compile time over a lowered [`MirModule`] with zero
//! runtime overhead:
//! 1. Identifies secret sources: parameters or named locals whose names match
//!    `secret*`, `sk`/`sk_*`/`sk<digit>*`, `key`/`key_*`/`*_key`/`key<digit>*`,
//!    `nonce*`, `priv*`, plus all parameters of functions matching `ct_*`.
//! 2. Propagates secret taint monotonically to fixed point across intra- and
//!    inter-procedural MIR dataflow (assignments, ALU/bitwise ops, casts,
//!    aggregates, memory/field loads & stores, call arguments, return values,
//!    and mutable pointer out-parameters), recording deterministic shortest
//!    taint-flow traces.
//! 3. Fails closed whenever any secret-tainted value reaches a timing sink:
//!    - Secret-dependent branch or switch condition (`secret_dependent_branch`),
//!    - Secret-dependent array/memory index operand (`secret_dependent_memory_index`),
//!    - Variable-latency integer division or modulo (`secret_dependent_division`).
//! 4. Emits a canonical deterministic JSON manifest (`buildlang.ct-manifest/v1`)
//!    sealed with `manifest_sha256`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::codegen::analysis::cfg::reachable_blocks;
use crate::codegen::ir::{
    BinOp, MirFunction, MirLocal, MirModule, MirPlace, MirRValue, MirStmtKind, MirTerminator,
    MirType, MirValue, PlaceProjection,
};

/// Canonical schema identifier for `--ct-manifest-json`.
pub const CT_MANIFEST_SCHEMA: &str = "buildlang.ct-manifest/v1";

/// Side-channel sink violation category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CtViolationKind {
    /// Secret-tainted value controls a conditional branch or switch target.
    SecretDependentBranch,
    /// Secret-tainted value indexes an array or memory access (cache-timing leak).
    SecretDependentMemoryIndex,
    /// Secret-tainted value enters a variable-latency integer division or remainder.
    SecretDependentDivision,
}

/// Fail-closed constant-time violation record with exact taint-flow trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CtViolation {
    /// Function containing the timing side-channel sink.
    pub function: String,
    /// Side-channel leak category.
    pub kind: CtViolationKind,
    /// MIR basic block ID where the sink occurs.
    pub block_id: u32,
    /// Sink operation identifier (e.g. `if_cond`, `index_access`, `binop:div`).
    pub sink_op: String,
    /// Tainted local name or SSA slot reaching the sink.
    pub tainted_local: String,
    /// Ordered taint propagation trace from secret source to sink.
    pub taint_trace: Vec<String>,
    /// Human-readable fail-closed diagnostic message.
    pub message: String,
}

/// Per-function constant-time verification summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCtRecord {
    /// Function name.
    pub name: String,
    /// Whether the function matches the `ct_*` constant-time contract prefix.
    pub is_ct_annotated: bool,
    /// Direct secret source parameter/local labels in this function.
    pub secret_sources: Vec<String>,
    /// Number of tainted MIR locals after interprocedural fixed-point closure.
    pub tainted_locals: usize,
    /// True if zero side-channel violations were found in this function.
    pub constant_time: bool,
    /// Number of timing sink violations detected in this function.
    pub violation_count: usize,
}

/// Canonical module-level constant-time verification manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CtManifest {
    /// Schema tag (`buildlang.ct-manifest/v1`).
    pub schema: String,
    /// Audited module name or source path.
    pub module: String,
    /// True when zero constant-time side-channel violations occurred.
    pub constant_time_verified: bool,
    /// Total secret sources seeded across the module.
    pub secret_source_count: usize,
    /// Number of functions containing at least one tainted local.
    pub tainted_function_count: usize,
    /// Total side-channel violations detected.
    pub violation_count: usize,
    /// True iff `violation_count == 0`.
    pub passed: bool,
    /// Sorted per-function constant-time records.
    pub functions: Vec<FunctionCtRecord>,
    /// Sorted fail-closed side-channel violations.
    pub violations: Vec<CtViolation>,
    /// Lowercase hex SHA-256 over the canonical JSON payload (excluding this field).
    pub manifest_sha256: String,
}

#[derive(Serialize)]
struct CtManifestHashPayload<'a> {
    schema: &'a str,
    module: &'a str,
    constant_time_verified: bool,
    secret_source_count: usize,
    tainted_function_count: usize,
    violation_count: usize,
    passed: bool,
    functions: &'a [FunctionCtRecord],
    violations: &'a [CtViolation],
}

/// Recompute the canonical SHA-256 digest of a [`CtManifest`].
pub fn compute_ct_manifest_sha256(manifest: &CtManifest) -> Result<String, String> {
    let payload = CtManifestHashPayload {
        schema: &manifest.schema,
        module: &manifest.module,
        constant_time_verified: manifest.constant_time_verified,
        secret_source_count: manifest.secret_source_count,
        tainted_function_count: manifest.tainted_function_count,
        violation_count: manifest.violation_count,
        passed: manifest.passed,
        functions: &manifest.functions,
        violations: &manifest.violations,
    };
    let bytes =
        serde_json::to_vec(&payload).map_err(|e| format!("ct manifest serialize error: {e}"))?;
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

/// True if an identifier denotes a cryptographic secret source.
///
/// Matches `secret*`, `sk` / `sk_*` / `sk<digit>*` (without false-matching `skip`),
/// `key` / `key_*` / `*_key` / `key<digit>*`, `nonce*`, and `priv*`.
pub fn is_secret_identifier(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("secret") || lower.starts_with("nonce") || lower.starts_with("priv") {
        return true;
    }
    if lower == "sk"
        || lower.starts_with("sk_")
        || lower.ends_with("_sk")
        || (lower.starts_with("sk")
            && lower[2..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit()))
    {
        return true;
    }
    if lower == "key"
        || lower.starts_with("key_")
        || lower.ends_with("_key")
        || (lower.starts_with("key")
            && lower[3..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit()))
    {
        return true;
    }
    false
}

/// True if a function name carries the `ct_*` constant-time contract prefix.
pub fn is_ct_function_name(name: &str) -> bool {
    name.starts_with("ct_")
}

fn local_label(func: &MirFunction, id: u32) -> String {
    func.locals
        .iter()
        .find(|l| l.id.0 == id)
        .and_then(|l| l.name.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| format!("_{id}"))
}

fn value_taint<'a>(
    val: &MirValue,
    taint: &'a BTreeMap<u32, Vec<String>>,
) -> Option<(u32, &'a Vec<String>)> {
    match val {
        MirValue::Local(id) => taint.get(&id.0).map(|tr| (id.0, tr)),
        _ => None,
    }
}

fn place_taint<'a>(
    place: &MirPlace,
    taint: &'a BTreeMap<u32, Vec<String>>,
) -> Option<(u32, &'a Vec<String>)> {
    if let Some(tr) = taint.get(&place.local.0) {
        return Some((place.local.0, tr));
    }
    for proj in &place.projections {
        if let PlaceProjection::Index(idx) = proj {
            if let Some(tr) = taint.get(&idx.0) {
                return Some((idx.0, tr));
            }
        }
    }
    None
}

fn rvalue_taint<'a>(
    rv: &MirRValue,
    taint: &'a BTreeMap<u32, Vec<String>>,
) -> Option<(u32, &'a Vec<String>)> {
    match rv {
        MirRValue::Use(v)
        | MirRValue::UnaryOp { operand: v, .. }
        | MirRValue::Cast { value: v, .. }
        | MirRValue::Repeat { value: v, .. }
        | MirRValue::FieldAccess { base: v, .. }
        | MirRValue::VariantField { base: v, .. }
        | MirRValue::Deref { ptr: v, .. } => value_taint(v, taint),
        MirRValue::BinaryOp { left, right, .. } => {
            value_taint(left, taint).or_else(|| value_taint(right, taint))
        }
        MirRValue::Ref { place, .. }
        | MirRValue::AddressOf { place, .. }
        | MirRValue::Discriminant(place)
        | MirRValue::Len(place) => place_taint(place, taint),
        MirRValue::Aggregate { operands, .. } => {
            for op in operands {
                if let Some(hit) = value_taint(op, taint) {
                    return Some(hit);
                }
            }
            None
        }
        MirRValue::IndexAccess { base, index, .. } => {
            value_taint(base, taint).or_else(|| value_taint(index, taint))
        }
        MirRValue::TextureSample {
            texture,
            sampler,
            coords,
        } => value_taint(texture, taint)
            .or_else(|| value_taint(sampler, taint))
            .or_else(|| value_taint(coords, taint)),
        MirRValue::AddressOfGlobal { .. } | MirRValue::NullaryOp(_, _) => None,
    }
}

fn extend_trace(base: &[String], step: String) -> Vec<String> {
    let mut out = base.to_vec();
    if out.last() != Some(&step) && out.len() < 32 {
        out.push(step);
    }
    out
}

fn insert_if_new(map: &mut BTreeMap<u32, Vec<String>>, id: u32, trace: Vec<String>) -> bool {
    if map.contains_key(&id) {
        false
    } else {
        map.insert(id, trace);
        true
    }
}

/// Ordered parameter `LocalId`s of a function.
fn param_locals(func: &MirFunction) -> Vec<&MirLocal> {
    func.locals.iter().filter(|l| l.is_param).collect()
}

/// Run the compile-time constant-time cryptography and side-channel taint
/// verifier over a lowered [`MirModule`].
pub fn audit_mir_constant_time(
    module: &MirModule,
    module_label: &str,
) -> Result<CtManifest, String> {
    let mut fn_map: BTreeMap<String, &MirFunction> = BTreeMap::new();
    for f in &module.functions {
        fn_map.insert(f.name.to_string(), f);
    }

    // Per-function local taint map: local_id -> deterministic trace from secret source.
    let mut fn_taint: BTreeMap<String, BTreeMap<u32, Vec<String>>> = BTreeMap::new();
    let mut fn_sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Track alias targets of `ptr = Ref(place) / AddressOf(place)` for mut-ref writeback.
    let mut fn_ptr_aliases: BTreeMap<String, BTreeMap<u32, u32>> = BTreeMap::new();

    // 1. Seed secret sources deterministically.
    let mut total_secret_sources = 0usize;
    for (fname, func) in &fn_map {
        let mut taint = BTreeMap::new();
        let mut sources = Vec::new();
        let is_ct = is_ct_function_name(fname);

        for local in &func.locals {
            let lname = local.name.as_deref().unwrap_or("");
            let is_param_secret =
                local.is_param && (is_ct || (!lname.is_empty() && is_secret_identifier(lname)));
            let is_local_secret =
                !local.is_param && !lname.is_empty() && is_secret_identifier(lname);
            if is_param_secret || is_local_secret {
                let lbl = local_label(func, local.id.0);
                let tag = if local.is_param {
                    format!("param:{fname}::{lbl}")
                } else {
                    format!("local:{fname}::{lbl}")
                };
                taint.insert(local.id.0, vec![tag.clone()]);
                sources.push(tag);
                total_secret_sources += 1;
            }
        }
        sources.sort();
        sources.dedup();
        fn_taint.insert(fname.clone(), taint);
        fn_sources.insert(fname.clone(), sources);

        // Precompute direct local pointer aliases (`_p = &local`).
        let mut aliases = BTreeMap::new();
        if let Some(blocks) = &func.blocks {
            let reach = reachable_blocks(blocks);
            for (bi, b) in blocks.iter().enumerate() {
                if !reach[bi] {
                    continue;
                }
                for stmt in &b.stmts {
                    if let MirStmtKind::Assign {
                        dest,
                        value: MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. },
                    } = &stmt.kind
                    {
                        aliases.insert(dest.0, place.local.0);
                    }
                }
            }
        }
        fn_ptr_aliases.insert(fname.clone(), aliases);
    }

    // 2. Interprocedural monotonic fixed-point taint propagation.
    let mut fn_return_taint: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let max_iters = (fn_map.len() + 2).saturating_mul(16).max(32);

    for _ in 0..max_iters {
        let mut changed = false;

        for (fname, func) in &fn_map {
            let Some(blocks) = &func.blocks else {
                continue;
            };
            let reach = reachable_blocks(blocks);
            let aliases = fn_ptr_aliases.get(fname).cloned().unwrap_or_default();

            // Intra-procedural propagation within `fname`.
            loop {
                let mut local_changed = false;
                let cur_taint = fn_taint.get(fname).cloned().unwrap_or_default();
                let mut updates: Vec<(u32, Vec<String>)> = Vec::new();

                for (bi, block) in blocks.iter().enumerate() {
                    if !reach[bi] {
                        continue;
                    }
                    for stmt in &block.stmts {
                        match &stmt.kind {
                            MirStmtKind::Assign { dest, value } => {
                                if let Some((src_id, tr)) = rvalue_taint(value, &cur_taint) {
                                    let step = format!(
                                        "{fname}:{}->{}",
                                        local_label(func, src_id),
                                        local_label(func, dest.0)
                                    );
                                    updates.push((dest.0, extend_trace(tr, step)));
                                }
                            }
                            MirStmtKind::FieldAssign { base, value, .. } => {
                                if let Some((src_id, tr)) = rvalue_taint(value, &cur_taint) {
                                    let step = format!(
                                        "{fname}:{}->{}",
                                        local_label(func, src_id),
                                        local_label(func, base.0)
                                    );
                                    updates.push((base.0, extend_trace(tr, step)));
                                }
                            }
                            MirStmtKind::DerefAssign { ptr, value }
                            | MirStmtKind::FieldDerefAssign { ptr, value, .. } => {
                                if let Some((src_id, tr)) = rvalue_taint(value, &cur_taint) {
                                    let step = format!(
                                        "{fname}:{}->*{}",
                                        local_label(func, src_id),
                                        local_label(func, ptr.0)
                                    );
                                    let next_tr = extend_trace(tr, step);
                                    updates.push((ptr.0, next_tr.clone()));
                                    if let Some(&target) = aliases.get(&ptr.0) {
                                        updates.push((target, next_tr));
                                    }
                                }
                            }
                            MirStmtKind::IndexStore { base, value, .. } => {
                                if let MirValue::Local(base_id) = base {
                                    if let Some((src_id, tr)) = rvalue_taint(value, &cur_taint) {
                                        let step = format!(
                                            "{fname}:{}->{}[]",
                                            local_label(func, src_id),
                                            local_label(func, base_id.0)
                                        );
                                        let next_tr = extend_trace(tr, step);
                                        updates.push((base_id.0, next_tr.clone()));
                                        if let Some(&target) = aliases.get(&base_id.0) {
                                            updates.push((target, next_tr));
                                        }
                                    }
                                }
                            }
                            MirStmtKind::GlobalStore { .. }
                            | MirStmtKind::WorkgroupBarrier
                            | MirStmtKind::StorageLive(_)
                            | MirStmtKind::StorageDead(_)
                            | MirStmtKind::Nop => {}
                        }
                    }

                    // Propagate return-value taint out of `fname`.
                    if let Some(MirTerminator::Return(Some(ret_val))) = &block.terminator {
                        if let Some((_, tr)) = value_taint(ret_val, &cur_taint) {
                            if !fn_return_taint.contains_key(fname) {
                                fn_return_taint.insert(
                                    fname.clone(),
                                    extend_trace(tr, format!("ret:{fname}")),
                                );
                                changed = true;
                            }
                        }
                    }
                }

                let entry = fn_taint.entry(fname.clone()).or_default();
                for (id, tr) in updates {
                    if insert_if_new(entry, id, tr) {
                        local_changed = true;
                        changed = true;
                    }
                }
                if !local_changed {
                    break;
                }
            }

            // Interprocedural call edges from `fname` to callees.
            let caller_taint = fn_taint.get(fname).cloned().unwrap_or_default();
            let mut caller_dest_updates: Vec<(u32, Vec<String>)> = Vec::new();
            let mut callee_param_updates: Vec<(String, u32, Vec<String>)> = Vec::new();

            for (bi, block) in blocks.iter().enumerate() {
                if !reach[bi] {
                    continue;
                }
                if let Some(MirTerminator::Call {
                    func: callee_val,
                    args,
                    dest,
                    ..
                }) = &block.terminator
                {
                    let Some(callee_name) = (match callee_val {
                        MirValue::Function(n) | MirValue::Global(n) => Some(n.as_ref()),
                        _ => None,
                    }) else {
                        continue;
                    };

                    if let Some(callee_fn) = fn_map.get(callee_name) {
                        let cparams = param_locals(callee_fn);
                        // 2a. Caller args -> Callee params.
                        for (idx, arg) in args.iter().enumerate() {
                            if let (Some(cparam), Some((arg_id, tr))) =
                                (cparams.get(idx), value_taint(arg, &caller_taint))
                            {
                                let step = format!(
                                    "call:{fname}({})->{callee_name}({})",
                                    local_label(func, arg_id),
                                    local_label(callee_fn, cparam.id.0)
                                );
                                callee_param_updates.push((
                                    callee_name.to_string(),
                                    cparam.id.0,
                                    extend_trace(tr, step),
                                ));
                            }
                        }
                        // 2b. Callee return value -> Caller dest.
                        if let (Some(d), Some(ret_tr)) = (dest, fn_return_taint.get(callee_name)) {
                            let step =
                                format!("ret:{callee_name}->{fname}({})", local_label(func, d.0));
                            caller_dest_updates.push((d.0, extend_trace(ret_tr, step)));
                        }
                        // 2c. Callee mut-ptr out-param taint -> Caller arg & alias target.
                        if let Some(ctaint) = fn_taint.get(callee_name) {
                            for (idx, arg) in args.iter().enumerate() {
                                if let (Some(cparam), MirValue::Local(arg_id)) =
                                    (cparams.get(idx), arg)
                                {
                                    if matches!(cparam.ty, MirType::Ptr(_)) {
                                        if let Some(ctr) = ctaint.get(&cparam.id.0) {
                                            let step = format!(
                                                "out:{callee_name}({})->{fname}({})",
                                                local_label(callee_fn, cparam.id.0),
                                                local_label(func, arg_id.0)
                                            );
                                            let out_tr = extend_trace(ctr, step);
                                            caller_dest_updates.push((arg_id.0, out_tr.clone()));
                                            if let Some(&target) = aliases.get(&arg_id.0) {
                                                caller_dest_updates.push((target, out_tr));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if let Some(d) = dest {
                        // Pure/external helper call: conservatively propagate taint
                        // from any tainted argument to the return slot.
                        for arg in args {
                            if let Some((arg_id, tr)) = value_taint(arg, &caller_taint) {
                                let step = format!(
                                    "call:{callee_name}({})->{}",
                                    local_label(func, arg_id),
                                    local_label(func, d.0)
                                );
                                caller_dest_updates.push((d.0, extend_trace(tr, step)));
                                break;
                            }
                        }
                    }
                }
            }

            let entry = fn_taint.entry(fname.clone()).or_default();
            for (id, tr) in caller_dest_updates {
                if insert_if_new(entry, id, tr) {
                    changed = true;
                }
            }
            for (cname, pid, tr) in callee_param_updates {
                let centry = fn_taint.entry(cname).or_default();
                if insert_if_new(centry, pid, tr) {
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    // 3. Sink audit across all reachable blocks of every defined function.
    let mut violations: Vec<CtViolation> = Vec::new();
    let mut records: Vec<FunctionCtRecord> = Vec::with_capacity(fn_map.len());
    let mut tainted_function_count = 0usize;

    for (fname, func) in &fn_map {
        let taint = fn_taint.get(fname).cloned().unwrap_or_default();
        if !taint.is_empty() {
            tainted_function_count += 1;
        }
        let mut fn_viols: Vec<CtViolation> = Vec::new();
        let mut seen_keys: BTreeSet<(CtViolationKind, u32, String, String)> = BTreeSet::new();

        let mut record_violation =
            |kind: CtViolationKind, block_id: u32, sink_op: &str, local_id: u32, tr: &[String]| {
                let lbl = local_label(func, local_id);
                let key = (kind, block_id, sink_op.to_string(), lbl.clone());
                if !seen_keys.insert(key) {
                    return;
                }
                let trace = extend_trace(tr, format!("sink:{fname}@bb{block_id}:{sink_op}({lbl})"));
                let kind_label = match kind {
                    CtViolationKind::SecretDependentBranch => "secret-dependent branch",
                    CtViolationKind::SecretDependentMemoryIndex => {
                        "secret-dependent memory/cache index"
                    }
                    CtViolationKind::SecretDependentDivision => {
                        "secret-dependent variable-latency division/modulo"
                    }
                };
                let message = format!(
                    "{kind_label} in `{fname}` at bb{block_id} ({sink_op} on `{lbl}`): {}",
                    trace.join(" -> ")
                );
                fn_viols.push(CtViolation {
                    function: fname.clone(),
                    kind,
                    block_id,
                    sink_op: sink_op.to_string(),
                    tainted_local: lbl,
                    taint_trace: trace,
                    message,
                });
            };

        if let Some(blocks) = &func.blocks {
            let reach = reachable_blocks(blocks);
            for (bi, block) in blocks.iter().enumerate() {
                if !reach[bi] {
                    continue;
                }
                let bid = block.id.0;

                for stmt in &block.stmts {
                    // Check IndexStore index operand.
                    if let MirStmtKind::IndexStore { index, .. } = &stmt.kind {
                        if let Some((idx_id, tr)) = value_taint(index, &taint) {
                            record_violation(
                                CtViolationKind::SecretDependentMemoryIndex,
                                bid,
                                "index_store",
                                idx_id,
                                tr,
                            );
                        }
                    }

                    // Inspect rvalue sinks (IndexAccess, PlaceProjection::Index, Div/Rem).
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
                        match rv {
                            MirRValue::IndexAccess { index, .. } => {
                                if let Some((idx_id, tr)) = value_taint(index, &taint) {
                                    record_violation(
                                        CtViolationKind::SecretDependentMemoryIndex,
                                        bid,
                                        "index_access",
                                        idx_id,
                                        tr,
                                    );
                                }
                            }
                            MirRValue::Ref { place, .. }
                            | MirRValue::AddressOf { place, .. }
                            | MirRValue::Discriminant(place)
                            | MirRValue::Len(place) => {
                                for proj in &place.projections {
                                    if let PlaceProjection::Index(idx) = proj {
                                        if let Some(tr) = taint.get(&idx.0) {
                                            record_violation(
                                                CtViolationKind::SecretDependentMemoryIndex,
                                                bid,
                                                "place_index",
                                                idx.0,
                                                tr,
                                            );
                                        }
                                    }
                                }
                            }
                            MirRValue::BinaryOp {
                                op: op @ (BinOp::Div | BinOp::Rem),
                                left,
                                right,
                            } => {
                                let sink = if matches!(op, BinOp::Div) {
                                    "binop:div"
                                } else {
                                    "binop:rem"
                                };
                                if let Some((lid, tr)) = value_taint(left, &taint) {
                                    record_violation(
                                        CtViolationKind::SecretDependentDivision,
                                        bid,
                                        sink,
                                        lid,
                                        tr,
                                    );
                                }
                                if let Some((rid, tr)) = value_taint(right, &taint) {
                                    record_violation(
                                        CtViolationKind::SecretDependentDivision,
                                        bid,
                                        sink,
                                        rid,
                                        tr,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Check branch / switch terminators.
                match &block.terminator {
                    Some(MirTerminator::If { cond, .. }) => {
                        if let Some((cid, tr)) = value_taint(cond, &taint) {
                            record_violation(
                                CtViolationKind::SecretDependentBranch,
                                bid,
                                "if_cond",
                                cid,
                                tr,
                            );
                        }
                    }
                    Some(MirTerminator::Switch { value, .. }) => {
                        if let Some((vid, tr)) = value_taint(value, &taint) {
                            record_violation(
                                CtViolationKind::SecretDependentBranch,
                                bid,
                                "switch_val",
                                vid,
                                tr,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        let vcount = fn_viols.len();
        violations.extend(fn_viols);
        records.push(FunctionCtRecord {
            name: fname.clone(),
            is_ct_annotated: is_ct_function_name(fname),
            secret_sources: fn_sources.get(fname).cloned().unwrap_or_default(),
            tainted_locals: taint.len(),
            constant_time: vcount == 0,
            violation_count: vcount,
        });
    }

    let violation_count = violations.len();
    let passed = violation_count == 0;

    let mut manifest = CtManifest {
        schema: CT_MANIFEST_SCHEMA.to_string(),
        module: module_label.to_string(),
        constant_time_verified: passed,
        secret_source_count: total_secret_sources,
        tainted_function_count,
        violation_count,
        passed,
        functions: records,
        violations,
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = compute_ct_manifest_sha256(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{BlockId, LocalId, MirBlock, MirConst, MirFnSig, MirStmt};
    use std::sync::Arc;

    #[test]
    fn secret_identifier_rules_avoid_skip_false_positive() {
        assert!(is_secret_identifier("secret_key"));
        assert!(is_secret_identifier("sk"));
        assert!(is_secret_identifier("sk_bytes"));
        assert!(is_secret_identifier("sk0"));
        assert!(is_secret_identifier("key"));
        assert!(is_secret_identifier("key_word"));
        assert!(is_secret_identifier("session_key"));
        assert!(is_secret_identifier("nonce_lo"));
        assert!(is_secret_identifier("priv_exp"));
        assert!(!is_secret_identifier("skip"));
        assert!(!is_secret_identifier("skipped_count"));
        assert!(!is_secret_identifier("idx"));
    }

    #[test]
    fn synthetic_interprocedural_secret_div_and_branch_fail_closed() {
        let mut m = MirModule::new("ct_unit");

        // helper(x: i64) -> i64 { x / 3 }
        let mut helper = MirFunction::new(
            "helper",
            MirFnSig::new(vec![MirType::i64()], MirType::i64()),
        );
        let mut p0 = MirLocal::named(LocalId(0), "x", MirType::i64());
        p0.is_param = true;
        helper.add_local(p0);
        helper.add_local(MirLocal::new(LocalId(1), MirType::i64()));
        let mut hbb = MirBlock::new(BlockId(0));
        hbb.push_stmt(MirStmt::assign(
            LocalId(1),
            MirRValue::BinaryOp {
                op: BinOp::Div,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Const(MirConst::Int(3, MirType::i64())),
            },
        ));
        hbb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        helper.add_block(hbb);
        m.add_function(helper);

        // main(secret_x: i64) -> i64 { helper(secret_x) }
        let mut main_fn =
            MirFunction::new("main", MirFnSig::new(vec![MirType::i64()], MirType::i64()));
        let mut mp0 = MirLocal::named(LocalId(0), "secret_x", MirType::i64());
        mp0.is_param = true;
        main_fn.add_local(mp0);
        main_fn.add_local(MirLocal::new(LocalId(1), MirType::i64()));
        let mut mbb0 = MirBlock::new(BlockId(0));
        mbb0.set_terminator(MirTerminator::Call {
            func: MirValue::Function(Arc::from("helper")),
            args: vec![MirValue::Local(LocalId(0))],
            dest: Some(LocalId(1)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut mbb1 = MirBlock::new(BlockId(1));
        mbb1.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        main_fn.add_block(mbb0);
        main_fn.add_block(mbb1);
        m.add_function(main_fn);

        let manifest = audit_mir_constant_time(&m, "ct_unit.bld").unwrap();
        assert!(!manifest.passed);
        assert!(!manifest.constant_time_verified);
        assert_eq!(manifest.violation_count, 1);
        assert_eq!(
            manifest.violations[0].kind,
            CtViolationKind::SecretDependentDivision
        );
        assert_eq!(manifest.violations[0].function, "helper");
        assert!(manifest.violations[0]
            .taint_trace
            .iter()
            .any(|s| s.contains("param:main::secret_x")));
        assert_eq!(
            compute_ct_manifest_sha256(&manifest).unwrap(),
            manifest.manifest_sha256
        );
    }

    #[test]
    fn constant_time_masked_bitops_pass_clean() {
        let mut m = MirModule::new("ct_clean");
        let mut f = MirFunction::new(
            "ct_cmov",
            MirFnSig::new(
                vec![MirType::u64(), MirType::u64(), MirType::u64()],
                MirType::u64(),
            ),
        );
        for (i, n) in [(0, "mask"), (1, "a"), (2, "b")] {
            let mut p = MirLocal::named(LocalId(i), n, MirType::u64());
            p.is_param = true;
            f.add_local(p);
        }
        f.add_local(MirLocal::new(LocalId(3), MirType::u64()));
        f.add_local(MirLocal::new(LocalId(4), MirType::u64()));
        f.add_local(MirLocal::new(LocalId(5), MirType::u64()));
        let mut bb = MirBlock::new(BlockId(0));
        bb.push_stmt(MirStmt::assign(
            LocalId(3),
            MirRValue::BinaryOp {
                op: BinOp::BitXor,
                left: MirValue::Local(LocalId(1)),
                right: MirValue::Local(LocalId(2)),
            },
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(4),
            MirRValue::BinaryOp {
                op: BinOp::BitAnd,
                left: MirValue::Local(LocalId(3)),
                right: MirValue::Local(LocalId(0)),
            },
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(5),
            MirRValue::BinaryOp {
                op: BinOp::BitXor,
                left: MirValue::Local(LocalId(2)),
                right: MirValue::Local(LocalId(4)),
            },
        ));
        bb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(5)))));
        f.add_block(bb);
        m.add_function(f);

        let manifest = audit_mir_constant_time(&m, "ct_clean.bld").unwrap();
        assert!(manifest.passed);
        assert!(manifest.constant_time_verified);
        assert_eq!(manifest.violation_count, 0);
        assert_eq!(manifest.secret_source_count, 3);
    }
}
