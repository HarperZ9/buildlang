// ===============================================================================
// BUILDLANG COMPILER - COMPILE-TIME BOUNDED-STACK & STATIC WCET MIR VERIFIER
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Interprocedural MIR bounded-stack and static WCET verifier.
//!
//! Operates strictly at compile time over a lowered [`MirModule`] with zero
//! runtime overhead:
//! 1. Computes exact 64-bit ABI size and alignment for every MIR type
//!    (scalars, fixed arrays, structs, enums, unions, tuples, slices, vectors,
//!    options, and pointers).
//! 2. Computes each function's static stack-frame byte estimate (ABI saved
//!    header + aligned named locals + unmoved aggregate temporaries +
//!    call-crossing scalar spill slots, rounded to 16-byte ABI alignment).
//! 3. Computes dominator-based natural loop nesting depth and static MIR
//!    instruction-cost / acyclic-path WCET bounds per function.
//! 4. Performs deterministic DFS cycle detection on the call graph to catch
//!    direct and mutual recursion fail-closed, then propagates transitive
//!    worst-case stack depth and WCET bounds along the heaviest call chain.
//! 5. Emits a deterministic canonical JSON manifest
//!    (`buildlang.wcet-manifest/v1`) sealed with `manifest_sha256`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::codegen::analysis::cfg::{
    block_id_index, reachable_blocks, stmt_uses_local, terminator_successors, terminator_uses_local,
};
use crate::codegen::ir::{
    BinOp, FloatSize, IntSize, MirBlock, MirFunction, MirModule, MirRValue, MirStmtKind,
    MirTerminator, MirType, MirTypeDef, MirValue, TypeDefKind,
};

/// Canonical schema identifier for `--wcet-manifest-json`.
pub const WCET_MANIFEST_SCHEMA: &str = "buildlang.wcet-manifest/v1";

/// Conservative static iteration multiplier applied per natural loop nesting level.
pub const DEFAULT_STATIC_LOOP_BOUND: u64 = 16;

/// Target ABI pointer width in bytes (LP64 / x86-64 / RV64 / AArch64).
const PTR_BYTES: u64 = 8;

/// Saved frame header bytes per defined function (saved FP + RA on 64-bit ABIs).
const ABI_FRAME_HEADER_BYTES: u64 = 16;

/// Number of general-purpose scalar registers available before spilling.
const SCALAR_REGISTER_BUDGET: usize = 12;

/// Violation category reported when a bounded-stack or recursion check fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WcetViolationKind {
    /// Transitive worst-case stack usage exceeded `--max-stack-bytes`.
    StackBudgetExceeded,
    /// Direct or mutual recursion creates an unbounded stack cycle.
    RecursionCycle,
}

/// Deterministic violation record with exact call-chain witness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WcetViolation {
    /// Violating entry or caller function.
    pub function: String,
    /// Violation kind (`stack_budget_exceeded` or `recursion_cycle`).
    pub kind: WcetViolationKind,
    /// Worst-case transitive stack bytes (`None` when unbounded due to recursion).
    pub stack_bytes: Option<u64>,
    /// Configured stack budget in bytes, if any.
    pub budget_bytes: Option<u64>,
    /// Ordered call-chain witness (`caller -> ... -> callee`).
    pub call_chain: Vec<String>,
    /// Per-frame stack bytes aligned with `call_chain` (0 for extern/cycle close).
    pub frame_bytes_chain: Vec<u64>,
    /// Human-readable fail-closed diagnostic message.
    pub message: String,
}

/// Per-function static stack-frame and WCET summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionWcetRecord {
    /// Function name.
    pub name: String,
    /// Whether this function is a declaration-only extern symbol.
    pub is_extern: bool,
    /// Local stack frame estimate in bytes (16-byte aligned).
    pub frame_bytes: u64,
    /// Transitive worst-case stack depth in bytes (`None` if part of/reaching recursion).
    pub max_stack_bytes: Option<u64>,
    /// Call chain achieving `max_stack_bytes` (or recursion cycle witness).
    pub max_stack_chain: Vec<String>,
    /// Local static MIR instruction cost (excluding callees, loop-weighted).
    pub local_wcet_cost: u64,
    /// Transitive worst-case MIR instruction cost (`None` if reaching recursion).
    pub transitive_wcet_cost: Option<u64>,
    /// Maximum natural loop nesting depth detected in this function's CFG.
    pub max_loop_depth: u32,
    /// Number of reachable basic blocks in this function.
    pub reachable_blocks: usize,
    /// Number of MIR statements across reachable blocks.
    pub mir_statements: usize,
    /// True if this function participates in or calls into a recursion cycle.
    pub has_recursion: bool,
    /// Sorted direct in-module callees.
    pub callees: Vec<String>,
}

/// Deterministic module-level bounded-stack and WCET verification manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WcetManifest {
    /// Schema tag (`buildlang.wcet-manifest/v1`).
    pub schema: String,
    /// Audited module name or source path.
    pub module: String,
    /// Optional configured `--max-stack-bytes` budget.
    pub max_stack_budget_bytes: Option<u64>,
    /// Static loop iteration multiplier used for loop-weighted WCET bounds.
    pub assumed_loop_bound: u64,
    /// Worst-case transitive stack bytes across all bounded functions in the module.
    pub module_max_stack_bytes: u64,
    /// Function achieving `module_max_stack_bytes` (if any bounded functions exist).
    pub heaviest_function: Option<String>,
    /// Worst-case transitive static WCET cost across bounded functions.
    pub module_max_wcet_cost: u64,
    /// True when no recursion cycles exist and all functions have finite static stack bounds.
    pub stack_bounded: bool,
    /// True when any function has a direct or mutual recursion cycle.
    pub has_recursion: bool,
    /// True when zero violations occurred (budget respected and recursion-free).
    pub passed: bool,
    /// Sorted per-function static stack and WCET records.
    pub functions: Vec<FunctionWcetRecord>,
    /// Sorted fail-closed violations (empty iff `passed == true`).
    pub violations: Vec<WcetViolation>,
    /// Lowercase hex SHA-256 over the canonical JSON payload (excluding this field).
    pub manifest_sha256: String,
}

#[derive(Serialize)]
struct WcetManifestHashPayload<'a> {
    schema: &'a str,
    module: &'a str,
    max_stack_budget_bytes: Option<u64>,
    assumed_loop_bound: u64,
    module_max_stack_bytes: u64,
    heaviest_function: &'a Option<String>,
    module_max_wcet_cost: u64,
    stack_bounded: bool,
    has_recursion: bool,
    passed: bool,
    functions: &'a [FunctionWcetRecord],
    violations: &'a [WcetViolation],
}

/// Recompute the canonical SHA-256 digest of a [`WcetManifest`].
pub fn compute_wcet_manifest_sha256(manifest: &WcetManifest) -> Result<String, String> {
    let payload = WcetManifestHashPayload {
        schema: &manifest.schema,
        module: &manifest.module,
        max_stack_budget_bytes: manifest.max_stack_budget_bytes,
        assumed_loop_bound: manifest.assumed_loop_bound,
        module_max_stack_bytes: manifest.module_max_stack_bytes,
        heaviest_function: &manifest.heaviest_function,
        module_max_wcet_cost: manifest.module_max_wcet_cost,
        stack_bounded: manifest.stack_bounded,
        has_recursion: manifest.has_recursion,
        passed: manifest.passed,
        functions: &manifest.functions,
        violations: &manifest.violations,
    };
    let bytes =
        serde_json::to_vec(&payload).map_err(|e| format!("wcet manifest serialize error: {e}"))?;
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

fn align_up(offset: u64, align: u64) -> u64 {
    let a = align.max(1);
    let rem = offset % a;
    if rem == 0 {
        offset
    } else {
        offset.saturating_add(a - rem)
    }
}

/// Compute `(size_bytes, align_bytes)` for a [`MirType`] under 64-bit LP64 ABI rules.
fn type_layout(
    ty: &MirType,
    typedefs: &BTreeMap<String, &MirTypeDef>,
    visiting: &mut BTreeSet<String>,
) -> (u64, u64) {
    match ty {
        MirType::Void | MirType::Never => (0, 1),
        MirType::Bool => (1, 1),
        MirType::Int(sz, _) => match sz {
            IntSize::I8 => (1, 1),
            IntSize::I16 => (2, 2),
            IntSize::I32 => (4, 4),
            IntSize::I64 | IntSize::ISize => (8, 8),
            IntSize::I128 => (16, 16),
        },
        MirType::Float(sz) => match sz {
            FloatSize::F32 => (4, 4),
            FloatSize::F64 => (8, 8),
        },
        MirType::Ptr(_)
        | MirType::FnPtr(_)
        | MirType::Vec(_)
        | MirType::Map(_, _)
        | MirType::Texture2D(_)
        | MirType::Sampler
        | MirType::SampledImage(_) => (PTR_BYTES, PTR_BYTES),
        MirType::Slice(_) | MirType::TraitObject(_) => (PTR_BYTES * 2, PTR_BYTES),
        MirType::Array(elem, count) => {
            let (es, ea) = type_layout(elem, typedefs, visiting);
            let stride = align_up(es, ea);
            (stride.saturating_mul(*count), ea)
        }
        MirType::Vector(elem, lanes) => {
            let (es, ea) = type_layout(elem, typedefs, visiting);
            let total = es.saturating_mul(u64::from(*lanes));
            let vec_align = total.next_power_of_two().clamp(ea, 16);
            (align_up(total, vec_align), vec_align)
        }
        MirType::Option(inner) => {
            let (is, ia) = type_layout(inner, typedefs, visiting);
            let off = align_up(1, ia);
            (align_up(off.saturating_add(is), ia), ia)
        }
        MirType::Tuple(elems) => struct_fields_layout(elems.iter(), false, typedefs, visiting),
        MirType::Struct(name) => {
            let key = name.as_ref();
            if !visiting.insert(key.to_string()) {
                return (PTR_BYTES, PTR_BYTES);
            }
            let res = if let Some(def) = typedefs.get(key) {
                match &def.kind {
                    TypeDefKind::Struct { fields, packed } => struct_fields_layout(
                        fields.iter().map(|(_, fty)| fty),
                        *packed,
                        typedefs,
                        visiting,
                    ),
                    TypeDefKind::Union { variants } => {
                        let mut max_sz = 0u64;
                        let mut max_al = 1u64;
                        for (_, vty) in variants {
                            let (vs, va) = type_layout(vty, typedefs, visiting);
                            max_sz = max_sz.max(vs);
                            max_al = max_al.max(va);
                        }
                        (align_up(max_sz, max_al), max_al)
                    }
                    TypeDefKind::Enum {
                        discriminant_ty,
                        variants,
                    } => {
                        let (ds, da) = type_layout(discriminant_ty, typedefs, visiting);
                        let mut max_pay_sz = 0u64;
                        let mut max_pay_al = 1u64;
                        for v in variants {
                            let (vs, va) = struct_fields_layout(
                                v.fields.iter().map(|(_, fty)| fty),
                                false,
                                typedefs,
                                visiting,
                            );
                            max_pay_sz = max_pay_sz.max(vs);
                            max_pay_al = max_pay_al.max(va);
                        }
                        let overall_align = da.max(max_pay_al);
                        let pay_off = align_up(ds, max_pay_al);
                        (
                            align_up(pay_off.saturating_add(max_pay_sz), overall_align),
                            overall_align,
                        )
                    }
                }
            } else {
                (PTR_BYTES, PTR_BYTES)
            };
            visiting.remove(key);
            res
        }
    }
}

fn struct_fields_layout<'a>(
    fields: impl Iterator<Item = &'a MirType>,
    packed: bool,
    typedefs: &BTreeMap<String, &MirTypeDef>,
    visiting: &mut BTreeSet<String>,
) -> (u64, u64) {
    let mut offset = 0u64;
    let mut max_align = 1u64;
    for fty in fields {
        let (fs, fa) = type_layout(fty, typedefs, visiting);
        let eff_align = if packed { 1 } else { fa };
        offset = align_up(offset, eff_align).saturating_add(fs);
        max_align = max_align.max(eff_align);
    }
    (align_up(offset, max_align), max_align)
}

/// Identify unnamed temporaries that are immediately moved into a named local
/// and never referenced elsewhere, so large array/struct initializers are not
/// double-counted on the stack frame.
fn find_single_move_init_temps(func: &MirFunction, blocks: &[MirBlock]) -> BTreeSet<u32> {
    let named_ids: BTreeSet<u32> = func
        .locals
        .iter()
        .filter(|l| l.name.is_some())
        .map(|l| l.id.0)
        .collect();

    let mut use_counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut moved_to_named: BTreeSet<u32> = BTreeSet::new();

    for local in &func.locals {
        if local.name.is_some() {
            continue;
        }
        let mut uses = 0usize;
        let mut moved = false;
        for b in blocks {
            for s in &b.stmts {
                if stmt_uses_local(&s.kind, local.id) {
                    uses += 1;
                    if let MirStmtKind::Assign {
                        dest,
                        value: MirRValue::Use(MirValue::Local(src)),
                    } = &s.kind
                    {
                        if *src == local.id && named_ids.contains(&dest.0) {
                            moved = true;
                        }
                    }
                }
            }
            if terminator_uses_local(&b.terminator, local.id) {
                uses += 1;
            }
        }
        use_counts.insert(local.id.0, uses);
        if uses == 1 && moved {
            moved_to_named.insert(local.id.0);
        }
    }

    let _ = use_counts;
    moved_to_named
}

/// Estimate static stack-frame bytes for a defined MIR function.
fn compute_frame_bytes(func: &MirFunction, typedefs: &BTreeMap<String, &MirTypeDef>) -> u64 {
    let Some(blocks) = &func.blocks else {
        return 0;
    };

    let mut visiting = BTreeSet::new();
    let single_move_temps = find_single_move_init_temps(func, blocks);

    // Collect address-taken locals (must reside in stack memory).
    let mut addr_taken: BTreeSet<u32> = BTreeSet::new();
    let mut has_call = false;
    for block in blocks {
        for stmt in &block.stmts {
            if let MirStmtKind::Assign { value, .. }
            | MirStmtKind::DerefAssign { value, .. }
            | MirStmtKind::FieldDerefAssign { value, .. }
            | MirStmtKind::FieldAssign { value, .. }
            | MirStmtKind::IndexStore { value, .. }
            | MirStmtKind::GlobalStore { value, .. } = &stmt.kind
            {
                if let MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } = value {
                    addr_taken.insert(place.local.0);
                }
            }
        }
        if matches!(block.terminator, Some(MirTerminator::Call { .. })) {
            has_call = true;
        }
    }

    let mut frame = ABI_FRAME_HEADER_BYTES;
    let mut unnamed_scalar_count = 0usize;

    for local in &func.locals {
        let (sz, al) = type_layout(&local.ty, typedefs, &mut visiting);
        if sz == 0 {
            continue;
        }
        if local.is_param {
            // Parameters > 16 B or address-taken parameters require stack space.
            if sz > 16 || addr_taken.contains(&local.id.0) {
                frame = align_up(frame, al).saturating_add(sz);
            }
            continue;
        }
        if local.name.is_some() || addr_taken.contains(&local.id.0) {
            frame = align_up(frame, al).saturating_add(sz);
        } else if sz > 16 {
            // Composite/aggregate temporary that does not fit in scalar registers.
            if !single_move_temps.contains(&local.id.0) {
                frame = align_up(frame, al).saturating_add(sz);
            }
        } else {
            unnamed_scalar_count += 1;
        }
    }

    // Account for register-pressure spill slots when scalar temporaries exceed
    // the hardware register budget in non-leaf or high-pressure functions.
    if unnamed_scalar_count > SCALAR_REGISTER_BUDGET {
        let excess = unnamed_scalar_count - SCALAR_REGISTER_BUDGET;
        // Conservative bounded spill slots: at most 8 bytes per 4 excess SSA temps,
        // or 8 bytes per 2 excess SSA temps when calls clobber caller-saved regs.
        let divisor = if has_call { 2 } else { 4 };
        let spill_slots = (excess.div_ceil(divisor)).min(32) as u64;
        frame = align_up(frame, 8).saturating_add(spill_slots * 8);
    }

    align_up(frame, 16)
}

/// Static MIR cost weight for one statement.
fn stmt_wcet_cost(kind: &MirStmtKind) -> u64 {
    match kind {
        MirStmtKind::Nop | MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) => 0,
        MirStmtKind::WorkgroupBarrier => 8,
        MirStmtKind::DerefAssign { value, .. }
        | MirStmtKind::FieldDerefAssign { value, .. }
        | MirStmtKind::IndexStore { value, .. }
        | MirStmtKind::GlobalStore { value, .. } => 2 + rvalue_wcet_cost(value),
        MirStmtKind::Assign { value, .. } | MirStmtKind::FieldAssign { value, .. } => {
            1 + rvalue_wcet_cost(value)
        }
    }
}

fn rvalue_wcet_cost(rv: &MirRValue) -> u64 {
    match rv {
        MirRValue::Use(_) | MirRValue::NullaryOp(_, _) | MirRValue::Len(_) => 0,
        MirRValue::UnaryOp { .. }
        | MirRValue::Ref { .. }
        | MirRValue::AddressOf { .. }
        | MirRValue::AddressOfGlobal { .. }
        | MirRValue::Cast { .. }
        | MirRValue::Discriminant(_)
        | MirRValue::FieldAccess { .. }
        | MirRValue::VariantField { .. } => 1,
        MirRValue::Deref { .. } | MirRValue::IndexAccess { .. } => 2,
        MirRValue::TextureSample { .. } => 8,
        MirRValue::Aggregate { operands, .. } => (operands.len() as u64).max(1),
        MirRValue::Repeat { count, .. } => (*count).min(256).max(1),
        MirRValue::BinaryOp { op, .. } => match op {
            BinOp::Div | BinOp::Rem | BinOp::Pow => 8,
            BinOp::Mul | BinOp::MulChecked | BinOp::MulWrapping | BinOp::MulSaturating => 3,
            _ => 1,
        },
    }
}

fn terminator_wcet_cost(term: &Option<MirTerminator>) -> u64 {
    match term {
        None | Some(MirTerminator::Unreachable) => 0,
        Some(MirTerminator::Call { .. }) => 4,
        Some(MirTerminator::Switch { targets, .. }) => 2 + (targets.len() as u64).min(8),
        Some(_) => 1,
    }
}

struct LocalCfgMetrics {
    local_wcet_cost: u64,
    max_loop_depth: u32,
    reachable_blocks: usize,
    mir_statements: usize,
}

/// Compute dominator-based natural loop nesting depth and loop-weighted longest
/// acyclic path static WCET cost for one function.
fn compute_cfg_wcet(blocks: &[MirBlock], assumed_loop_bound: u64) -> LocalCfgMetrics {
    let n = blocks.len();
    if n == 0 {
        return LocalCfgMetrics {
            local_wcet_cost: 0,
            max_loop_depth: 0,
            reachable_blocks: 0,
            mir_statements: 0,
        };
    }

    let id_to_idx = block_id_index(blocks);
    let entry = id_to_idx.get(&0).copied().unwrap_or(0);
    let reach = reachable_blocks(blocks);

    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, b) in blocks.iter().enumerate() {
        if !reach[i] {
            continue;
        }
        let mut s = terminator_successors(&b.terminator, &id_to_idx);
        s.sort_unstable();
        s.dedup();
        for &t in &s {
            if reach[t] {
                preds[t].push(i);
            }
        }
        succs[i] = s;
    }

    // Iterative dominator sets over reachable nodes.
    let all_reach: BTreeSet<usize> = (0..n).filter(|&i| reach[i]).collect();
    let mut dom: Vec<BTreeSet<usize>> = (0..n)
        .map(|i| {
            if !reach[i] {
                BTreeSet::new()
            } else if i == entry {
                BTreeSet::from([entry])
            } else {
                all_reach.clone()
            }
        })
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for &i in &all_reach {
            if i == entry {
                continue;
            }
            let mut pred_iter = preds[i].iter().copied().filter(|&p| reach[p]);
            let mut next_dom = match pred_iter.next() {
                Some(first) => dom[first].clone(),
                None => BTreeSet::new(),
            };
            for p in pred_iter {
                next_dom = next_dom.intersection(&dom[p]).copied().collect();
            }
            next_dom.insert(i);
            if next_dom != dom[i] {
                dom[i] = next_dom;
                changed = true;
            }
        }
    }

    // Natural loop back-edges: (tail -> header) where header dominates tail.
    let mut back_edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut loop_depth: Vec<u32> = vec![0; n];
    for &tail in &all_reach {
        for &header in &succs[tail] {
            if dom[tail].contains(&header) {
                back_edges.insert((tail, header));
                // Natural loop body = reverse BFS from tail stopping at header.
                let mut body = BTreeSet::from([header, tail]);
                let mut queue = VecDeque::new();
                if tail != header {
                    queue.push_back(tail);
                }
                while let Some(cur) = queue.pop_front() {
                    for &p in &preds[cur] {
                        if body.insert(p) && p != header {
                            queue.push_back(p);
                        }
                    }
                }
                for b in body {
                    loop_depth[b] = loop_depth[b].saturating_add(1);
                }
            }
        }
    }

    let max_loop_depth = loop_depth.iter().copied().max().unwrap_or(0);
    let mut mir_statements = 0usize;
    let mut block_cost: Vec<u64> = vec![0; n];
    for &i in &all_reach {
        mir_statements += blocks[i].stmts.len();
        let raw_stmts: u64 = blocks[i]
            .stmts
            .iter()
            .map(|s| stmt_wcet_cost(&s.kind))
            .sum();
        let raw = raw_stmts.saturating_add(terminator_wcet_cost(&blocks[i].terminator));
        let factor = assumed_loop_bound.saturating_pow(loop_depth[i].min(6));
        block_cost[i] = raw.saturating_mul(factor);
    }

    // Longest path on the DAG obtained by dropping back-edges.
    // Guard against irreducible CFGs via Kahn topological sort with cycle break.
    let mut in_deg = vec![0usize; n];
    let mut dag_succs: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &u in &all_reach {
        for &v in &succs[u] {
            if reach[v] && !back_edges.contains(&(u, v)) && u != v {
                dag_succs[u].push(v);
                in_deg[v] += 1;
            }
        }
    }

    let mut topo = Vec::with_capacity(all_reach.len());
    let mut ready: BTreeSet<usize> = all_reach
        .iter()
        .copied()
        .filter(|&i| in_deg[i] == 0)
        .collect();
    let mut visited = vec![false; n];
    while let Some(u) = ready.pop_first() {
        visited[u] = true;
        topo.push(u);
        for &v in &dag_succs[u] {
            in_deg[v] = in_deg[v].saturating_sub(1);
            if in_deg[v] == 0 {
                ready.insert(v);
            }
        }
    }
    // Include any nodes trapped in irreducible cycles deterministically.
    for &i in &all_reach {
        if !visited[i] {
            topo.push(i);
        }
    }

    let mut best_path: Vec<u64> = vec![0; n];
    best_path[entry] = block_cost[entry];
    let mut reachable_in_dag = vec![false; n];
    reachable_in_dag[entry] = true;
    for &u in &topo {
        if !reachable_in_dag[u] {
            continue;
        }
        let u_cost = best_path[u];
        for &v in &dag_succs[u] {
            reachable_in_dag[v] = true;
            let cand = u_cost.saturating_add(block_cost[v]);
            if cand > best_path[v] {
                best_path[v] = cand;
            }
        }
    }

    let local_wcet_cost = all_reach
        .iter()
        .copied()
        .map(|i| best_path[i].max(block_cost[i]))
        .max()
        .unwrap_or(0);

    LocalCfgMetrics {
        local_wcet_cost,
        max_loop_depth,
        reachable_blocks: all_reach.len(),
        mir_statements,
    }
}

/// Extract sorted direct in-module callees from a function's reachable blocks.
fn extract_callees(func: &MirFunction, all_fn_names: &BTreeSet<String>) -> Vec<String> {
    let Some(blocks) = &func.blocks else {
        return Vec::new();
    };
    let reach = reachable_blocks(blocks);
    let mut callees = BTreeSet::new();
    for (i, b) in blocks.iter().enumerate() {
        if !reach[i] {
            continue;
        }
        if let Some(MirTerminator::Call { func: callee, .. }) = &b.terminator {
            if let MirValue::Function(name) | MirValue::Global(name) = callee {
                let cname = name.as_ref();
                if all_fn_names.contains(cname) {
                    callees.insert(cname.to_string());
                }
            }
        }
    }
    callees.into_iter().collect()
}

/// Run the compile-time bounded-stack and static WCET verifier over a lowered [`MirModule`].
pub fn audit_mir_wcet_stack(
    module: &MirModule,
    module_label: &str,
    max_stack_budget_bytes: Option<u64>,
) -> Result<WcetManifest, String> {
    if let Some(0) = max_stack_budget_bytes {
        return Err("invalid --max-stack-bytes value `0`: budget must be >= 1 byte".to_string());
    }

    let typedefs: BTreeMap<String, &MirTypeDef> = module
        .types
        .iter()
        .map(|td| (td.name.to_string(), td))
        .collect();

    let mut fn_map: BTreeMap<String, &MirFunction> = BTreeMap::new();
    for f in &module.functions {
        fn_map.insert(f.name.to_string(), f);
    }
    let all_fn_names: BTreeSet<String> = fn_map.keys().cloned().collect();

    // 1. Local per-function frame size, CFG WCET, and direct callees.
    let mut frame_bytes: BTreeMap<String, u64> = BTreeMap::new();
    let mut cfg_metrics: BTreeMap<String, LocalCfgMetrics> = BTreeMap::new();
    let mut callees_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (name, func) in &fn_map {
        frame_bytes.insert(name.clone(), compute_frame_bytes(func, &typedefs));
        let metrics = match &func.blocks {
            Some(blocks) => compute_cfg_wcet(blocks, DEFAULT_STATIC_LOOP_BOUND),
            None => LocalCfgMetrics {
                local_wcet_cost: 0,
                max_loop_depth: 0,
                reachable_blocks: 0,
                mir_statements: 0,
            },
        };
        cfg_metrics.insert(name.clone(), metrics);
        callees_map.insert(name.clone(), extract_callees(func, &all_fn_names));
    }

    // 2. Transitive DFS computing exact worst-case stack chain, WCET cost, and
    //    fail-closed cycle witnesses for direct or mutual recursion.
    #[derive(Clone)]
    enum EvalResult {
        Bounded {
            stack_bytes: u64,
            stack_chain: Vec<String>,
            wcet_cost: u64,
        },
        Cyclic {
            cycle_chain: Vec<String>,
        },
    }

    fn eval_fn(
        name: &str,
        frame_bytes: &BTreeMap<String, u64>,
        cfg_metrics: &BTreeMap<String, LocalCfgMetrics>,
        callees_map: &BTreeMap<String, Vec<String>>,
        path: &mut Vec<String>,
        memo: &mut BTreeMap<String, EvalResult>,
    ) -> EvalResult {
        if let Some(pos) = path.iter().position(|p| p == name) {
            let mut cycle = path[pos..].to_vec();
            cycle.push(name.to_string());
            return EvalResult::Cyclic { cycle_chain: cycle };
        }
        if let Some(cached) = memo.get(name) {
            return cached.clone();
        }

        path.push(name.to_string());
        let local_frame = frame_bytes.get(name).copied().unwrap_or(0);
        let local_wcet = cfg_metrics.get(name).map_or(0, |m| m.local_wcet_cost);

        let mut best_child_stack = 0u64;
        let mut best_child_chain: Vec<String> = Vec::new();
        let mut best_child_wcet = 0u64;
        let mut found_cycle: Option<Vec<String>> = None;

        if let Some(callees) = callees_map.get(name) {
            for callee in callees {
                match eval_fn(callee, frame_bytes, cfg_metrics, callees_map, path, memo) {
                    EvalResult::Cyclic { cycle_chain } => {
                        if found_cycle.is_none() {
                            let witness = if cycle_chain.iter().any(|node| node == name) {
                                cycle_chain
                            } else {
                                let mut w = vec![name.to_string()];
                                w.extend(cycle_chain);
                                w
                            };
                            found_cycle = Some(witness);
                        }
                    }
                    EvalResult::Bounded {
                        stack_bytes,
                        stack_chain,
                        wcet_cost,
                    } => {
                        if stack_bytes > best_child_stack
                            || (stack_bytes == best_child_stack && best_child_chain.is_empty())
                        {
                            best_child_stack = stack_bytes;
                            best_child_chain = stack_chain;
                        }
                        if wcet_cost > best_child_wcet {
                            best_child_wcet = wcet_cost;
                        }
                    }
                }
            }
        }

        path.pop();

        let res = if let Some(cycle_chain) = found_cycle {
            EvalResult::Cyclic { cycle_chain }
        } else {
            let mut chain = vec![name.to_string()];
            chain.extend(best_child_chain);
            EvalResult::Bounded {
                stack_bytes: local_frame.saturating_add(best_child_stack),
                stack_chain: chain,
                wcet_cost: local_wcet.saturating_add(best_child_wcet),
            }
        };

        // Only memoize non-cyclic results (or cycles when path is empty) so
        // callers outside a cycle still get the full caller->cycle witness.
        if matches!(res, EvalResult::Bounded { .. }) || path.is_empty() {
            memo.insert(name.to_string(), res.clone());
        }
        res
    }

    let mut memo: BTreeMap<String, EvalResult> = BTreeMap::new();
    let mut records = Vec::with_capacity(fn_map.len());
    let mut violations = Vec::new();
    let mut module_max_stack_bytes = 0u64;
    let mut heaviest_function: Option<String> = None;
    let mut module_max_wcet_cost = 0u64;
    let mut any_recursion = false;

    for (name, func) in &fn_map {
        let mut path = Vec::new();
        let res = eval_fn(
            name,
            &frame_bytes,
            &cfg_metrics,
            &callees_map,
            &mut path,
            &mut memo,
        );
        let lf = frame_bytes.get(name).copied().unwrap_or(0);
        let cm = &cfg_metrics[name];
        let callees = callees_map.get(name).cloned().unwrap_or_default();

        match res {
            EvalResult::Cyclic { cycle_chain } => {
                any_recursion = true;
                let frame_chain: Vec<u64> = cycle_chain
                    .iter()
                    .map(|f| frame_bytes.get(f).copied().unwrap_or(0))
                    .collect();
                let trace = cycle_chain.join(" -> ");
                violations.push(WcetViolation {
                    function: name.clone(),
                    kind: WcetViolationKind::RecursionCycle,
                    stack_bytes: None,
                    budget_bytes: max_stack_budget_bytes,
                    call_chain: cycle_chain.clone(),
                    frame_bytes_chain: frame_chain,
                    message: format!(
                        "unbounded stack due to recursion cycle reachable from `{name}` via {trace}"
                    ),
                });
                records.push(FunctionWcetRecord {
                    name: name.clone(),
                    is_extern: func.blocks.is_none(),
                    frame_bytes: lf,
                    max_stack_bytes: None,
                    max_stack_chain: cycle_chain,
                    local_wcet_cost: cm.local_wcet_cost,
                    transitive_wcet_cost: None,
                    max_loop_depth: cm.max_loop_depth,
                    reachable_blocks: cm.reachable_blocks,
                    mir_statements: cm.mir_statements,
                    has_recursion: true,
                    callees,
                });
            }
            EvalResult::Bounded {
                stack_bytes,
                stack_chain,
                wcet_cost,
            } => {
                if stack_bytes > module_max_stack_bytes
                    || (stack_bytes == module_max_stack_bytes && heaviest_function.is_none())
                {
                    module_max_stack_bytes = stack_bytes;
                    heaviest_function = Some(name.clone());
                }
                if wcet_cost > module_max_wcet_cost {
                    module_max_wcet_cost = wcet_cost;
                }
                if let Some(budget) = max_stack_budget_bytes {
                    if stack_bytes > budget {
                        let frame_chain: Vec<u64> = stack_chain
                            .iter()
                            .map(|f| frame_bytes.get(f).copied().unwrap_or(0))
                            .collect();
                        let trace = stack_chain
                            .iter()
                            .zip(frame_chain.iter())
                            .map(|(f, b)| format!("{f}({b}B)"))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        violations.push(WcetViolation {
                            function: name.clone(),
                            kind: WcetViolationKind::StackBudgetExceeded,
                            stack_bytes: Some(stack_bytes),
                            budget_bytes: Some(budget),
                            call_chain: stack_chain.clone(),
                            frame_bytes_chain: frame_chain,
                            message: format!(
                                "stack budget exceeded in `{name}`: {stack_bytes} B > {budget} B via {trace}"
                            ),
                        });
                    }
                }
                records.push(FunctionWcetRecord {
                    name: name.clone(),
                    is_extern: func.blocks.is_none(),
                    frame_bytes: lf,
                    max_stack_bytes: Some(stack_bytes),
                    max_stack_chain: stack_chain,
                    local_wcet_cost: cm.local_wcet_cost,
                    transitive_wcet_cost: Some(wcet_cost),
                    max_loop_depth: cm.max_loop_depth,
                    reachable_blocks: cm.reachable_blocks,
                    mir_statements: cm.mir_statements,
                    has_recursion: false,
                    callees,
                });
            }
        }
    }

    let stack_bounded = !any_recursion;
    let passed = violations.is_empty();

    let mut manifest = WcetManifest {
        schema: WCET_MANIFEST_SCHEMA.to_string(),
        module: module_label.to_string(),
        max_stack_budget_bytes,
        assumed_loop_bound: DEFAULT_STATIC_LOOP_BOUND,
        module_max_stack_bytes,
        heaviest_function,
        module_max_wcet_cost,
        stack_bounded,
        has_recursion: any_recursion,
        passed,
        functions: records,
        violations,
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = compute_wcet_manifest_sha256(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{BlockId, LocalId, MirConst, MirFnSig, MirLocal, MirStmt};
    use std::sync::Arc;

    fn make_simple_fn(
        name: &str,
        locals: Vec<(&str, MirType)>,
        callee: Option<&str>,
    ) -> MirFunction {
        let mut f = MirFunction::new(name, MirFnSig::new(vec![], MirType::i32()));
        for (lname, lty) in locals {
            let id = LocalId(f.locals.len() as u32);
            f.add_local(MirLocal::named(id, lname, lty));
        }
        let mut bb0 = MirBlock::new(BlockId(0));
        if let Some(c) = callee {
            bb0.set_terminator(MirTerminator::Call {
                func: MirValue::Function(Arc::from(c)),
                args: vec![],
                dest: None,
                target: Some(BlockId(1)),
                unwind: None,
            });
            let mut bb1 = MirBlock::new(BlockId(1));
            bb1.set_terminator(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
                0,
                MirType::i32(),
            )))));
            f.add_block(bb0);
            f.add_block(bb1);
        } else {
            bb0.push_stmt(MirStmt::nop());
            bb0.set_terminator(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
                0,
                MirType::i32(),
            )))));
            f.add_block(bb0);
        }
        f
    }

    #[test]
    fn oversized_array_exceeds_stack_budget_with_exact_chain() {
        let mut m = MirModule::new("oversized");
        m.add_function(make_simple_fn(
            "leaf_big",
            vec![("buf", MirType::Array(Box::new(MirType::i64()), 320))],
            None,
        ));
        m.add_function(make_simple_fn(
            "mid",
            vec![("x", MirType::i64())],
            Some("leaf_big"),
        ));
        m.add_function(make_simple_fn("main", vec![], Some("mid")));

        let manifest = audit_mir_wcet_stack(&m, "oversized.bld", Some(2048)).unwrap();
        assert!(!manifest.passed);
        assert!(manifest.stack_bounded);
        assert!(!manifest.has_recursion);
        assert!(manifest.module_max_stack_bytes >= 2560);
        let main_v = manifest
            .violations
            .iter()
            .find(|v| v.function == "main")
            .expect("main violation");
        assert_eq!(main_v.kind, WcetViolationKind::StackBudgetExceeded);
        assert_eq!(main_v.call_chain, vec!["main", "mid", "leaf_big"]);
        assert_eq!(
            compute_wcet_manifest_sha256(&manifest).unwrap(),
            manifest.manifest_sha256
        );
    }

    #[test]
    fn mutual_recursion_fails_closed_with_exact_cycle_witness() {
        let mut m = MirModule::new("mutual");
        m.add_function(make_simple_fn("ping", vec![], Some("pong")));
        m.add_function(make_simple_fn("pong", vec![], Some("ping")));
        m.add_function(make_simple_fn("main", vec![], Some("ping")));

        let manifest = audit_mir_wcet_stack(&m, "mutual.bld", Some(2048)).unwrap();
        assert!(!manifest.passed);
        assert!(!manifest.stack_bounded);
        assert!(manifest.has_recursion);
        let main_v = manifest
            .violations
            .iter()
            .find(|v| v.function == "main")
            .expect("main recursion violation");
        assert_eq!(main_v.kind, WcetViolationKind::RecursionCycle);
        assert_eq!(main_v.call_chain, vec!["main", "ping", "pong", "ping"]);
    }

    #[test]
    fn zero_budget_rejected_fail_closed() {
        let m = MirModule::new("empty");
        assert!(audit_mir_wcet_stack(&m, "empty.bld", Some(0)).is_err());
    }
}
