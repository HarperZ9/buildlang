//! Forward must-dataflow: the set of (local, bound) facts that hold at the
//! start of each block on every path from the entry.
use super::*;

type Facts = HashSet<(LocalId, Bound)>;

fn address_taken(func: &MirFunction) -> HashSet<LocalId> {
    let mut out = HashSet::new();
    for b in func.blocks.iter().flatten() {
        for st in &b.stmts {
            if let MirStmtKind::Assign { value, .. } = &st.kind {
                if let MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } = value {
                    out.insert(place.local);
                }
            }
        }
    }
    out
}

/// The facts an edge adds when `cond` (defined in `block` as a comparison)
/// is true (`taken = true`) or false.
fn edge_facts(
    func: &MirFunction,
    defs: &HashMap<LocalId, &MirRValue>,
    cond: &MirValue,
    taken: bool,
) -> Facts {
    let mut f = Facts::new();
    let MirValue::Local(c) = cond else { return f };
    let Some(MirRValue::BinaryOp { op, left, right }) = defs.get(c) else {
        return f;
    };
    // Normalise to `a < b` holding on this edge.
    let lt = match (op, taken) {
        (BinOp::Lt, true) | (BinOp::Ge, false) => Some((left, right)),
        (BinOp::Gt, true) | (BinOp::Le, false) => Some((right, left)),
        _ => None,
    };
    if let Some((a, b)) = lt {
        if let MirValue::Local(x) = a {
            if comparable(func, *x, b) {
                f.insert((*x, Bound::BelowMax));
            }
        }
        if let MirValue::Local(x) = b {
            if comparable(func, *x, a) {
                f.insert((*x, Bound::AboveMin));
            }
        }
    }
    f
}

pub(super) fn claims(func: &MirFunction, plant: bool) -> Vec<Site> {
    let blocks = func.blocks.as_ref().expect("checked by caller");
    let index: HashMap<u32, usize> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.0, i))
        .collect();
    let taken_addr = address_taken(func);
    // None = not reached yet (top of the lattice).
    let mut entry: Vec<Option<Facts>> = vec![None; blocks.len()];
    if !blocks.is_empty() {
        entry[0] = Some(Facts::new());
    }
    let mut changed = true;
    while changed {
        changed = false;
        for (i, b) in blocks.iter().enumerate() {
            let Some(mut facts) = entry[i].clone() else {
                continue;
            };
            let mut defs: HashMap<LocalId, &MirRValue> = HashMap::new();
            for st in &b.stmts {
                if let MirStmtKind::Assign { dest, value } = &st.kind {
                    facts.retain(|(x, _)| x != dest);
                    defs.retain(|_, v| !mentions(v, *dest));
                    defs.insert(*dest, value);
                }
            }
            let mut out: Vec<(usize, Facts)> = Vec::new();
            match &b.terminator {
                Some(MirTerminator::Goto(t)) => out.push((index[&t.0], facts.clone())),
                Some(MirTerminator::If {
                    cond,
                    then_block,
                    else_block,
                }) if then_block != else_block => {
                    let mut t = facts.clone();
                    t.extend(edge_facts(func, &defs, cond, true));
                    let mut e = facts.clone();
                    e.extend(edge_facts(func, &defs, cond, false));
                    out.push((index[&then_block.0], t));
                    out.push((index[&else_block.0], e));
                }
                Some(MirTerminator::If { then_block, .. }) => {
                    out.push((index[&then_block.0], facts.clone()))
                }
                Some(MirTerminator::Switch { .. }) => {
                    for t in super::checker::successors(b) {
                        out.push((index[&t], facts.clone()));
                    }
                }
                Some(MirTerminator::Call { dest, target, .. }) => {
                    if let Some(d) = dest {
                        facts.retain(|(x, _)| x != d);
                    }
                    if let Some(t) = target {
                        out.push((index[&t.0], facts.clone()));
                    }
                }
                _ => {}
            }
            for (j, f) in out {
                let f: Facts = f
                    .into_iter()
                    .filter(|(x, _)| !taken_addr.contains(x))
                    .collect();
                let merged = match &entry[j] {
                    None => f,
                    Some(old) => old.intersection(&f).copied().collect(),
                };
                if entry[j].as_ref() != Some(&merged) {
                    entry[j] = Some(merged);
                    changed = true;
                }
            }
        }
    }
    let mut sites = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let Some(mut facts) = entry[i].clone() else {
            continue;
        };
        for (s, st) in b.stmts.iter().enumerate() {
            if let Some((x, bound)) = step_at(func, &st.kind) {
                if plant || facts.contains(&(x, bound)) {
                    sites.push((i, s));
                }
            }
            if let MirStmtKind::Assign { dest, .. } = &st.kind {
                facts.retain(|(x, _)| x != dest);
            }
        }
    }
    sites
}

fn mentions(v: &MirRValue, x: LocalId) -> bool {
    match v {
        MirRValue::BinaryOp { left, right, .. } => {
            matches!(left, MirValue::Local(l) if *l == x)
                || matches!(right, MirValue::Local(r) if *r == x)
        }
        _ => false,
    }
}
