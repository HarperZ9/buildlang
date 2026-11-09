//! Backward search: every path that reaches the step passes the true edge
//! of a guard on `x` with no assignment to `x` after it.
use super::*;

pub(super) fn successors(b: &crate::codegen::ir::MirBlock) -> Vec<u32> {
    match &b.terminator {
        Some(MirTerminator::Goto(t)) => vec![t.0],
        Some(MirTerminator::If {
            then_block,
            else_block,
            ..
        }) => vec![then_block.0, else_block.0],
        Some(MirTerminator::Switch {
            targets, default, ..
        }) => {
            let mut v: Vec<u32> = targets.iter().map(|(_, t)| t.0).collect();
            v.push(default.0);
            v
        }
        Some(MirTerminator::Call {
            target: Some(t), ..
        }) => vec![t.0],
        _ => vec![],
    }
}

fn assigns(kind: &MirStmtKind, x: LocalId) -> bool {
    matches!(kind, MirStmtKind::Assign { dest, .. } if *dest == x)
}

fn addressed(func: &MirFunction, x: LocalId) -> bool {
    func.blocks.iter().flatten().flat_map(|b| &b.stmts).any(|st| {
        matches!(&st.kind, MirStmtKind::Assign { value: MirRValue::Ref { place: MirPlace { local, .. }, .. } | MirRValue::AddressOf { place: MirPlace { local, .. }, .. }, .. } if *local == x)
    })
}

/// Whether the edge from block `p` to block id `to` is the edge on which
/// a guard in `p` makes `bound` hold for `x`.
fn guard_edge(
    func: &MirFunction,
    p: &crate::codegen::ir::MirBlock,
    to: u32,
    x: LocalId,
    bound: Bound,
) -> bool {
    let Some(MirTerminator::If {
        cond: MirValue::Local(c),
        then_block,
        else_block,
    }) = &p.terminator
    else {
        return false;
    };
    if then_block.0 == else_block.0 {
        return false;
    }
    let taken = then_block.0 == to;
    // The last definition of `c` in `p`, with neither `c` nor `x`
    // assigned after it.
    let mut def = None;
    for st in p.stmts.iter().rev() {
        if let MirStmtKind::Assign { dest, value } = &st.kind {
            if *dest == *c {
                def = Some(value);
                break;
            }
            if *dest == x {
                return false;
            }
        }
    }
    let Some(MirRValue::BinaryOp { op, left, right }) = def else {
        return false;
    };
    let is_x = |v: &MirValue| matches!(v, MirValue::Local(l) if *l == x);
    // The comparison that is true on this edge, as (smaller, larger).
    let (small, large) = match (op, taken) {
        (BinOp::Lt, true) | (BinOp::Ge, false) => (left, right),
        (BinOp::Gt, true) | (BinOp::Le, false) => (right, left),
        _ => return false,
    };
    match bound {
        Bound::BelowMax => is_x(small) && comparable(func, x, large),
        Bound::AboveMin => is_x(large) && comparable(func, x, small),
    }
}

pub(super) fn holds(func: &MirFunction, b: usize, s: usize, x: LocalId, bound: Bound) -> bool {
    let Some(blocks) = func.blocks.as_ref() else {
        return false;
    };
    if addressed(func, x) {
        return false;
    }
    if blocks[b].stmts[..s].iter().any(|st| assigns(&st.kind, x)) {
        return false;
    }
    let preds = |id: u32| -> Vec<usize> {
        blocks
            .iter()
            .enumerate()
            .filter(|(_, p)| successors(p).contains(&id))
            .map(|(i, _)| i)
            .collect()
    };
    let mut visited: HashSet<usize> = HashSet::new();
    let mut stack: Vec<usize> = vec![b];
    visited.insert(b);
    while let Some(cur) = stack.pop() {
        if cur == 0 {
            return false; // reached the entry with no guard
        }
        for p in preds(blocks[cur].id.0) {
            if guard_edge(func, &blocks[p], blocks[cur].id.0, x, bound) {
                continue;
            }
            let kills = blocks[p].stmts.iter().any(|st| assigns(&st.kind, x))
                || matches!(&blocks[p].terminator, Some(MirTerminator::Call { dest: Some(d), .. }) if *d == x);
            if kills {
                return false;
            }
            if visited.insert(p) {
                stack.push(p);
            }
        }
    }
    true
}
