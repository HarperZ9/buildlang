//! CFG queries, exhaustive use-def scans, and move-chain tracking over MIR.
use std::collections::{HashMap, HashSet};

use crate::codegen::ir::{
    BlockId, LocalId, MirBlock, MirPlace, MirRValue, MirStmtKind, MirTerminator, MirValue,
    PlaceProjection,
};

/// The directly-named callee of a `Call`, if any.
pub(crate) fn callee_name(func: &MirValue) -> Option<&str> {
    match func {
        MirValue::Function(n) | MirValue::Global(n) => Some(n.as_ref()),
        _ => None,
    }
}

/// The move-source chain of `id`: every local whose buffer `id` ultimately
/// acquired through a chain of `dest = Use(src)` moves (`id`'s immediate
/// source, its source, ...). These locals alias `id`'s heap buffer, so a
/// `.ptr` borrow taken off ANY of them must also be confined to `id`'s block
/// before `id` is block-scoped freed. Closes the move-source borrow gap the
/// adversarial audit flagged (latent: not currently source-reachable).
pub(crate) fn move_source_chain(id: LocalId, blocks: &[MirBlock]) -> Vec<LocalId> {
    let mut chain = Vec::new();
    let mut cur = id;
    loop {
        let mut next = None;
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign {
                    dest,
                    value: MirRValue::Use(MirValue::Local(src)),
                } = &stmt.kind
                {
                    if *dest == cur {
                        next = Some(*src);
                    }
                }
            }
        }
        match next {
            Some(s) if !chain.contains(&s) && s != id => {
                chain.push(s);
                cur = s;
            }
            _ => break,
        }
    }
    chain
}

/// True if statement `kind` USES `x` (reads it). The `Assign` dest is a
/// definition, not a use, so it is excluded.
pub(crate) fn stmt_uses_local(kind: &MirStmtKind, x: LocalId) -> bool {
    match kind {
        MirStmtKind::Assign { value, .. } => rvalue_mentions(value, x),
        MirStmtKind::DerefAssign { ptr, value } => *ptr == x || rvalue_mentions(value, x),
        MirStmtKind::FieldDerefAssign { ptr, value, .. } => *ptr == x || rvalue_mentions(value, x),
        MirStmtKind::FieldAssign { base, value, .. } => *base == x || rvalue_mentions(value, x),
        MirStmtKind::IndexStore {
            base, index, value, ..
        } => {
            matches!(base, MirValue::Local(l) if *l == x)
                || matches!(index, MirValue::Local(l) if *l == x)
                || rvalue_mentions(value, x)
        }
        MirStmtKind::GlobalStore { value, .. } => rvalue_mentions(value, x),
        MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop => false,
    }
}

/// True if a terminator USES `x` (any appearance as a value or place). A
/// `Call` dest is a definition and is not represented here.
pub(crate) fn terminator_uses_local(term: &Option<MirTerminator>, x: LocalId) -> bool {
    let is = |v: &MirValue| matches!(v, MirValue::Local(l) if *l == x);
    match term {
        Some(MirTerminator::If { cond, .. }) => is(cond),
        Some(MirTerminator::Switch { value, .. }) => is(value),
        Some(MirTerminator::Call { func, args, .. }) => is(func) || args.iter().any(is),
        Some(MirTerminator::Return(Some(v))) => is(v),
        Some(MirTerminator::Assert { cond, .. }) => is(cond),
        Some(MirTerminator::Drop { place, .. }) => {
            place.local == x
                || place
                    .projections
                    .iter()
                    .any(|p| matches!(p, PlaceProjection::Index(l) if *l == x))
        }
        _ => false,
    }
}

/// Blocks reachable from the entry (`BlockId(0)` if present, else index 0).
pub(crate) fn reachable_blocks(blocks: &[MirBlock]) -> Vec<bool> {
    let id_to_index = block_id_index(blocks);
    let entry = id_to_index.get(&0).copied().unwrap_or(0);
    let mut seen = vec![false; blocks.len()];
    let mut stack = vec![entry];
    while let Some(i) = stack.pop() {
        if i >= blocks.len() || seen[i] {
            continue;
        }
        seen[i] = true;
        for s in terminator_successors(&blocks[i].terminator, &id_to_index) {
            stack.push(s);
        }
    }
    seen
}

/// Map each block's `BlockId` value to its index in `blocks`.
pub(crate) fn block_id_index(blocks: &[MirBlock]) -> HashMap<u32, usize> {
    blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.0, i))
        .collect()
}

/// Successor block indices of a terminator, resolved through `id_to_index`.
pub(crate) fn terminator_successors(
    term: &Option<MirTerminator>,
    id_to_index: &HashMap<u32, usize>,
) -> Vec<usize> {
    let resolve = |b: &BlockId| id_to_index.get(&b.0).copied();
    let mut out = Vec::new();
    match term {
        Some(MirTerminator::Goto(t)) => out.extend(resolve(t)),
        Some(MirTerminator::If {
            then_block,
            else_block,
            ..
        }) => {
            out.extend(resolve(then_block));
            out.extend(resolve(else_block));
        }
        Some(MirTerminator::Switch {
            targets, default, ..
        }) => {
            for (_, t) in targets {
                out.extend(resolve(t));
            }
            out.extend(resolve(default));
        }
        Some(MirTerminator::Call { target, unwind, .. }) => {
            out.extend(target.as_ref().and_then(&resolve));
            out.extend(unwind.as_ref().and_then(&resolve));
        }
        Some(MirTerminator::Drop { target, unwind, .. }) => {
            out.extend(resolve(target));
            out.extend(unwind.as_ref().and_then(&resolve));
        }
        Some(MirTerminator::Assert { target, unwind, .. }) => {
            out.extend(resolve(target));
            out.extend(unwind.as_ref().and_then(&resolve));
        }
        // Return, Unreachable, Resume, Abort, and None have no successors.
        _ => {}
    }
    out
}

/// Iterative dominator sets: `dom[i]` is the set of block indices that
/// dominate block `i`. `X` dominates `Y` iff `X` is in `dom[Y]`. Only
/// REACHABLE predecessors are intersected: the MIR lowering routinely emits
/// unreachable blocks, and an unreachable predecessor (with `dom = {itself}`)
/// would otherwise erase a join's true dominators. That erasure is fail-safe
/// (it only shrinks dom-sets, so dominance can spuriously FAIL, never
/// spuriously hold), but it silently suppresses most sound frees, so it is
/// fixed here.
pub(crate) fn compute_dominators(blocks: &[MirBlock]) -> Vec<HashSet<usize>> {
    let n = blocks.len();
    let id_to_index = block_id_index(blocks);
    let entry = id_to_index.get(&0).copied().unwrap_or(0);
    let reachable = reachable_blocks(blocks);
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, b) in blocks.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        for s in terminator_successors(&b.terminator, &id_to_index) {
            preds[s].push(i);
        }
    }
    let all: HashSet<usize> = (0..n).collect();
    let mut dom: Vec<HashSet<usize>> = vec![all; n];
    dom[entry] = std::iter::once(entry).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if i == entry || !reachable[i] {
                continue;
            }
            let mut new_set: Option<HashSet<usize>> = None;
            for &p in &preds[i] {
                new_set = Some(match new_set {
                    None => dom[p].clone(),
                    Some(acc) => acc.intersection(&dom[p]).copied().collect(),
                });
            }
            let mut new_set = new_set.unwrap_or_default();
            new_set.insert(i);
            if new_set != dom[i] {
                dom[i] = new_set;
                changed = true;
            }
        }
    }
    dom
}

/// True if `id` is mentioned anywhere in an rvalue, as a value or a place.
/// The exhaustive match makes the compiler enforce completeness: a new
/// `MirRValue` variant will not compile until handled here.
pub(crate) fn rvalue_mentions(r: &MirRValue, id: LocalId) -> bool {
    let v = |val: &MirValue| matches!(val, MirValue::Local(l) if *l == id);
    let p = |pl: &MirPlace| {
        pl.local == id
            || pl
                .projections
                .iter()
                .any(|pr| matches!(pr, PlaceProjection::Index(l) if *l == id))
    };
    match r {
        MirRValue::Use(x) => v(x),
        MirRValue::BinaryOp { left, right, .. } => v(left) || v(right),
        MirRValue::UnaryOp { operand, .. } => v(operand),
        MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } => p(place),
        MirRValue::Cast { value, .. } => v(value),
        MirRValue::Aggregate { operands, .. } => operands.iter().any(v),
        MirRValue::Repeat { value, .. } => v(value),
        MirRValue::Discriminant(place) | MirRValue::Len(place) => p(place),
        MirRValue::NullaryOp(..) => false,
        MirRValue::FieldAccess { base, .. } => v(base),
        MirRValue::VariantField { base, .. } => v(base),
        MirRValue::IndexAccess { base, index, .. } => v(base) || v(index),
        MirRValue::Deref { ptr, .. } => v(ptr),
        MirRValue::TextureSample {
            texture,
            sampler,
            coords,
        } => v(texture) || v(sampler) || v(coords),
    }
}
