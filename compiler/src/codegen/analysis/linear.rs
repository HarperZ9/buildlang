//! Increment 2b: the MIR affine/borrow checker core.
//!
//! A forward move/init dataflow over `#[linear]` locals, run to a fixpoint over
//! the CFG. It flags the SOUNDNESS-critical linear-resource violations that the
//! name-keyed AST tracker cannot follow through MIR places:
//!
//! - **use-after-move** (double-consume): a linear local moved while already
//!   `Moved` on the incoming path.
//! - **borrow-after-move**: a linear local READ (`Ref`/`AddressOf`/deref/field)
//!   while `Moved` (closes class 5).
//! - **move-out-of-shared-borrow**: a linear referent moved out through a
//!   `Deref` of a shared (`&`, `is_mut == false`) reference (closes classes 1
//!   direct form + 3 monomorphized).
//!
//! Pure function of MIR (`super::cfg` / `super::liveness` substrate + the 2a
//! linearity annotations and span side-table). No `TypeContext`.
//!
//! # Soundness disposition
//!
//! This is a CHECKER, so a missed error is unsound. The dataflow is
//! conservative in the sound direction: at a CFG merge a local is `Moved` if it
//! is `Moved` on ANY predecessor path (maybe-moved), so a value consumed on one
//! branch is treated as consumed after the join. When it is genuinely ambiguous
//! whether a linear appears in a MOVING position, we treat it as a move (missing
//! a move is unsound; over-reporting is a precision concern deferred to 2c). The
//! direct single-move / loop-rebind / borrow-only cases are pinned precise by
//! the unit tests so this conservatism does not regress safe code.

use std::collections::{HashMap, HashSet};

use crate::codegen::ir::{
    LocalId, MirBlock, MirFunction, MirPlace, MirRValue, MirStmtKind, MirTerminator, MirValue,
    PlaceProjection,
};
use crate::lexer::Span;
use crate::types::{TypeError, TypeErrorWithSpan};

use super::cfg::{block_id_index, reachable_blocks, terminator_successors};

/// Move/init state of a single linear local at a program point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MoveState {
    /// The local owns a live value (freshly defined or never consumed).
    Init,
    /// The local's value has been consumed (moved out).
    Moved,
}

/// Per-block move/init lattice: `state[block][local]`. Missing = `Init`.
type BlockState = HashMap<LocalId, MoveState>;

/// True iff local `id` is `#[linear]` (2a annotation: the `"linear"` marker in
/// `MirLocal.annotations`).
fn is_linear_local(func: &MirFunction, id: LocalId) -> bool {
    func.locals
        .iter()
        .find(|l| l.id == id)
        .is_some_and(|l| l.annotations.iter().any(|a| a.as_ref() == "linear"))
}

/// The `MirLocal.name` of `id`, best-effort, for diagnostics (`_N` fallback).
fn local_name(func: &MirFunction, id: LocalId) -> String {
    func.locals
        .iter()
        .find(|l| l.id == id)
        .and_then(|l| l.name.as_ref())
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("_{}", id.0))
}

/// Build the set of pointer locals that are SHARED (`&`, `is_mut == false`)
/// borrows OF A LINEAR local, by scanning statement provenance. We do NOT rely
/// on any annotation on the pointer local: real 2a lowering tags only linear
/// ADT `Struct` locals, never a `&Coin` binding (whose type is
/// `Ptr(Struct("Coin"))`). Instead we recover the borrow from its defining
/// statement:
///
///   `dest = &referent`      (`Ref { is_mut: false, place }`)
///   `dest = addr_of referent` (`AddressOf { is_mut: false, place }`)
///
/// where `place.local` is a linear local. Each such `dest` is a shared ref to a
/// linear value; a `Deref` of it is the illegal move-out-of-shared-borrow.
///
/// SCOPE: only a DIRECT borrow of a linear BASE local (`place.local` linear).
/// Borrowing a linear FIELD (`&obj.coin`) or a reborrow leaves `dest` out of
/// the set (its base local is not linear), which is correct for 2b: those are
/// 2c and are simply not flagged here.
fn shared_linear_ref_set(func: &MirFunction) -> HashSet<LocalId> {
    let mut set = HashSet::new();
    let Some(blocks) = &func.blocks else {
        return set;
    };
    for block in blocks {
        for stmt in &block.stmts {
            if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                let place = match value {
                    MirRValue::Ref {
                        is_mut: false,
                        place,
                    }
                    | MirRValue::AddressOf {
                        is_mut: false,
                        place,
                    } => place,
                    _ => continue,
                };
                // Only a borrow of the linear local itself (base, no
                // projections into a linear field) is the direct 2b case.
                if place.projections.is_empty() && is_linear_local(func, place.local) {
                    set.insert(*dest);
                }
            }
        }
    }
    set
}

/// Whether an rvalue is a MOVE-OUT-OF-SHARED-BORROW of a linear referent:
/// a `Deref { ptr }` (or a place with a leading `Deref` projection) whose
/// pointer local is a known shared borrow of a linear value (`shared_refs`,
/// built from borrow provenance). Returns the offending pointer local.
fn move_out_of_shared_borrow(value: &MirRValue, shared_refs: &HashSet<LocalId>) -> Option<LocalId> {
    match value {
        MirRValue::Deref {
            ptr: MirValue::Local(p),
            ..
        } if shared_refs.contains(p) => Some(*p),
        // A `Discriminant`/`Len` of a place with a leading `Deref` projection
        // is a place read through a borrow; a leading `Deref` of a shared
        // linear ref is the same move-out shape.
        MirRValue::Discriminant(place) | MirRValue::Len(place) => {
            if matches!(place.projections.first(), Some(PlaceProjection::Deref))
                && shared_refs.contains(&place.local)
            {
                Some(place.local)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Collect every linear local MOVED by an rvalue in an `Assign`. A move of
/// linear `L` = the rvalue transfers `L`'s value out: `Use(Local(L))`, or `L`
/// as an aggregate operand / cast / repeat operand (by-value composition). A
/// `Ref`/`AddressOf`/`FieldAccess`/`Deref` READS but does not move.
fn moved_by_rvalue(func: &MirFunction, value: &MirRValue, out: &mut Vec<LocalId>) {
    let mut push_if_linear = |v: &MirValue| {
        if let MirValue::Local(l) = v {
            if is_linear_local(func, *l) {
                out.push(*l);
            }
        }
    };
    match value {
        MirRValue::Use(v) => push_if_linear(v),
        // By-value composition consumes its operands. Conservative-sound: a
        // linear placed into an aggregate / repeated / cast by value is moved.
        MirRValue::Aggregate { operands, .. } => operands.iter().for_each(&mut push_if_linear),
        MirRValue::Repeat { value: v, .. } => push_if_linear(v),
        MirRValue::Cast { value: v, .. } => push_if_linear(v),
        // Borrows and reads do NOT move.
        MirRValue::Ref { .. }
        | MirRValue::AddressOf { .. }
        | MirRValue::FieldAccess { .. }
        | MirRValue::VariantField { .. }
        | MirRValue::IndexAccess { .. }
        | MirRValue::Deref { .. }
        | MirRValue::Discriminant(_)
        | MirRValue::Len(_)
        | MirRValue::BinaryOp { .. }
        | MirRValue::UnaryOp { .. }
        | MirRValue::NullaryOp(..)
        | MirRValue::TextureSample { .. } => {}
    }
}

/// Collect every linear local READ-BUT-NOT-MOVED by an rvalue: a borrow
/// (`Ref`/`AddressOf` whose place base is `L`), or a field / deref / index read
/// of `L`. These do not change move state, but if `L` is already `Moved` they
/// are a borrow-after-move.
fn read_by_rvalue(func: &MirFunction, value: &MirRValue, out: &mut Vec<LocalId>) {
    fn push_place(func: &MirFunction, place: &MirPlace, out: &mut Vec<LocalId>) {
        if is_linear_local(func, place.local) {
            out.push(place.local);
        }
    }
    fn push_val(func: &MirFunction, v: &MirValue, out: &mut Vec<LocalId>) {
        if let MirValue::Local(l) = v {
            if is_linear_local(func, *l) {
                out.push(*l);
            }
        }
    }
    match value {
        MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } => {
            push_place(func, place, out)
        }
        MirRValue::Discriminant(place) | MirRValue::Len(place) => push_place(func, place, out),
        MirRValue::FieldAccess { base, .. } | MirRValue::VariantField { base, .. } => {
            push_val(func, base, out)
        }
        MirRValue::IndexAccess { base, index, .. } => {
            push_val(func, base, out);
            push_val(func, index, out);
        }
        MirRValue::Deref { ptr, .. } => push_val(func, ptr, out),
        MirRValue::BinaryOp { left, right, .. } => {
            push_val(func, left, out);
            push_val(func, right, out);
        }
        MirRValue::UnaryOp { operand, .. } => push_val(func, operand, out),
        // A plain `Use`/aggregate/cast/repeat is a MOVE (handled by
        // `moved_by_rvalue`), not a non-consuming read.
        MirRValue::Use(_)
        | MirRValue::Aggregate { .. }
        | MirRValue::Repeat { .. }
        | MirRValue::Cast { .. }
        | MirRValue::NullaryOp(..)
        | MirRValue::TextureSample { .. } => {}
    }
}

/// Span for the statement at `(block_id, stmt_index)`, or `Span::dummy()`.
fn stmt_span(func: &MirFunction, block_id: u32, stmt_idx: usize) -> Span {
    func.spans
        .stmt
        .get(&(block_id, stmt_idx))
        .copied()
        .unwrap_or_else(Span::dummy)
}

/// Span for the terminator of block `block_id`, or `Span::dummy()`.
fn term_span(func: &MirFunction, block_id: u32) -> Span {
    func.spans
        .terminator
        .get(&block_id)
        .copied()
        .unwrap_or_else(Span::dummy)
}

/// Run the affine/borrow check over one MIR function. Pure: no mutation, no
/// side effects. Returns every violation with its statement-level span.
///
/// The transfer function walks each block forward from a per-block entry state
/// (the join of predecessor exit states). For every statement/terminator, in
/// order: process READS (borrow-after-move), MOVE-OUT-OF-BORROW, then MOVES
/// (use-after-move + `Init -> Moved`), then the DEFINITION reset (`-> Init`).
/// Iterating to a fixpoint over the CFG lets the maybe-moved join propagate
/// across back-edges and merges. Diagnostics are collected on a final,
/// deterministic forward pass over the converged entry states so each site is
/// reported at most once in stable order.
pub(crate) fn check(func: &MirFunction) -> Vec<TypeErrorWithSpan> {
    let blocks = match &func.blocks {
        Some(b) if !b.is_empty() => b.as_slice(),
        _ => return Vec::new(),
    };

    // If the function has no linear locals at all, nothing to check.
    if !func.locals.iter().any(|l| is_linear_local(func, l.id)) {
        return Vec::new();
    }

    let n = blocks.len();
    let id_to_index = block_id_index(blocks);
    let reachable = reachable_blocks(blocks);

    // Borrow-provenance pre-pass: which pointer locals are shared borrows of a
    // linear value. Computed once; used by move-out-of-shared-borrow detection.
    let shared_refs = shared_linear_ref_set(func);

    // Predecessor lists over reachable blocks only (an unreachable predecessor
    // contributes no real dataflow and would spuriously mark values moved).
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, b) in blocks.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        for s in terminator_successors(&b.terminator, &id_to_index) {
            if s < n {
                preds[s].push(i);
            }
        }
    }

    // Dataflow fixpoint: `entry[b]` / `exit[b]` are the move/init states at
    // each block boundary. Entry = maybe-moved join of predecessor exits.
    let mut entry: Vec<BlockState> = vec![BlockState::new(); n];
    let mut exit: Vec<BlockState> = vec![BlockState::new(); n];

    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if !reachable[i] {
                continue;
            }
            // Join predecessor exits: a local is Moved on entry if Moved on ANY
            // predecessor path (maybe-moved). Absent = Init, so the union takes
            // Moved whenever any predecessor recorded Moved.
            let mut new_entry = BlockState::new();
            for &p in &preds[i] {
                for (&local, &st) in &exit[p] {
                    let slot = new_entry.entry(local).or_insert(MoveState::Init);
                    if st == MoveState::Moved {
                        *slot = MoveState::Moved;
                    }
                }
            }
            if new_entry != entry[i] {
                entry[i] = new_entry;
                changed = true;
            }
            // Transfer through the block (no diagnostics on the fixpoint pass).
            let mut state = entry[i].clone();
            transfer_block(func, &blocks[i], &shared_refs, &mut state, None);
            if state != exit[i] {
                exit[i] = state;
                changed = true;
            }
        }
    }

    // Final diagnostic pass over the converged entry states, deterministic
    // block order. Only reachable blocks (unreachable code is never executed
    // and its "moves" are not real).
    let mut errors = Vec::new();
    for i in 0..n {
        if !reachable[i] {
            continue;
        }
        let mut state = entry[i].clone();
        transfer_block(
            func,
            &blocks[i],
            &shared_refs,
            &mut state,
            Some(&mut errors),
        );
    }
    errors
}

/// Apply the block's statements and terminator to `state` in order. When
/// `errors` is `Some`, emit diagnostics (use-after-move / borrow-after-move /
/// move-out-of-shared-borrow); when `None`, run silently (fixpoint pass).
fn transfer_block(
    func: &MirFunction,
    block: &MirBlock,
    shared_refs: &HashSet<LocalId>,
    state: &mut BlockState,
    mut errors: Option<&mut Vec<TypeErrorWithSpan>>,
) {
    let block_id = block.id.0;
    for (idx, stmt) in block.stmts.iter().enumerate() {
        let span = stmt_span(func, block_id, idx);
        apply_stmt(
            func,
            &stmt.kind,
            shared_refs,
            state,
            span,
            errors.as_deref_mut(),
        );
    }
    let span = term_span(func, block_id);
    apply_terminator(func, &block.terminator, state, span, errors.as_deref_mut());
}

/// Transfer for one statement. Order within a statement:
/// 1. move-out-of-shared-borrow (a distinct illegal SOURCE shape),
/// 2. reads (borrow-after-move) on the still-current state,
/// 3. moves (use-after-move, then `Init -> Moved`),
/// 4. the definition reset (`dest -> Init`), which happens AFTER the rvalue is
///    evaluated (so `L = Use(L)` reads/moves the old L, then rebinds fresh).
fn apply_stmt(
    func: &MirFunction,
    kind: &MirStmtKind,
    shared_refs: &HashSet<LocalId>,
    state: &mut BlockState,
    span: Span,
    mut errors: Option<&mut Vec<TypeErrorWithSpan>>,
) {
    if let MirStmtKind::Assign { dest, value } = kind {
        // (1) move-out-of-shared-borrow: `dest = *shared_ref_to_linear`.
        if let Some(ptr) = move_out_of_shared_borrow(value, shared_refs) {
            if let Some(errs) = errors.as_deref_mut() {
                errs.push(TypeErrorWithSpan::new(
                    TypeError::LinearMoveOutOfBorrow {
                        name: local_name(func, ptr),
                    },
                    span,
                ));
            }
        }

        // (2) reads: borrow-after-move on the current state.
        let mut reads = Vec::new();
        read_by_rvalue(func, value, &mut reads);
        for l in reads {
            if state.get(&l) == Some(&MoveState::Moved) {
                if let Some(errs) = errors.as_deref_mut() {
                    errs.push(TypeErrorWithSpan::new(
                        TypeError::LinearBorrowAfterMove {
                            name: local_name(func, l),
                        },
                        span,
                    ));
                }
            }
        }

        // (3) moves: use-after-move, then transition to Moved.
        let mut moves = Vec::new();
        moved_by_rvalue(func, value, &mut moves);
        for l in moves {
            apply_move(func, l, state, span, errors.as_deref_mut());
        }

        // (4) definition reset: the dest is (re)bound to a fresh value.
        if is_linear_local(func, *dest) {
            state.insert(*dest, MoveState::Init);
        }
        return;
    }

    // Non-Assign statements that touch a linear. A store's `value` can either
    // BORROW/READ a linear (borrow-after-move if already `Moved`) or CONSUME a
    // linear BY VALUE (a real move: `Use`/aggregate/cast/repeat operand). The
    // store target (`ptr`/`base`) is itself a borrow-position read of the
    // pointer/base local. We split the two:
    //   - `reads`: borrow-only touches -> borrow-after-move check, NO state
    //     change.
    //   - `moves`: by-value operands consumed into the store -> `apply_move`
    //     (use-after-move + `Init -> Moved`), so a linear moved through a store
    //     is a real consume (a subsequent use is then use-after-move, not a
    //     borrow-after-move).
    // A store target is not a linear rebind we track, so it never resets state.
    let mut reads = Vec::new();
    let mut moves = Vec::new();
    match kind {
        MirStmtKind::DerefAssign { ptr, value } => {
            if is_linear_local(func, *ptr) {
                reads.push(*ptr);
            }
            read_by_rvalue(func, value, &mut reads);
            moved_by_rvalue(func, value, &mut moves);
        }
        MirStmtKind::FieldDerefAssign { ptr, value, .. } => {
            if is_linear_local(func, *ptr) {
                reads.push(*ptr);
            }
            read_by_rvalue(func, value, &mut reads);
            moved_by_rvalue(func, value, &mut moves);
        }
        MirStmtKind::FieldAssign { base, value, .. } => {
            if is_linear_local(func, *base) {
                reads.push(*base);
            }
            read_by_rvalue(func, value, &mut reads);
            moved_by_rvalue(func, value, &mut moves);
        }
        MirStmtKind::GlobalStore { value, .. } => {
            read_by_rvalue(func, value, &mut reads);
            moved_by_rvalue(func, value, &mut moves);
        }
        MirStmtKind::Assign { .. }
        | MirStmtKind::StorageLive(_)
        | MirStmtKind::StorageDead(_)
        | MirStmtKind::Nop => {}
    }
    // Borrow-after-move on the current (pre-move) state for borrow-only touches.
    for l in reads {
        if state.get(&l) == Some(&MoveState::Moved) {
            if let Some(errs) = errors.as_deref_mut() {
                errs.push(TypeErrorWithSpan::new(
                    TypeError::LinearBorrowAfterMove {
                        name: local_name(func, l),
                    },
                    span,
                ));
            }
        }
    }
    // Then apply the by-value consumes (use-after-move + `Init -> Moved`).
    for l in moves {
        apply_move(func, l, state, span, errors.as_deref_mut());
    }
}

/// Transfer for a terminator. `Call` args passed BY VALUE move their linear
/// operands; `Return(Some(Local(L)))` moves `L`; the `Call` dest (re)binds.
/// `If`/`Switch`/`Assert` conditions READ their linear operand.
fn apply_terminator(
    func: &MirFunction,
    term: &Option<MirTerminator>,
    state: &mut BlockState,
    span: Span,
    mut errors: Option<&mut Vec<TypeErrorWithSpan>>,
) {
    match term {
        Some(MirTerminator::Call {
            args,
            dest,
            func: callee,
            ..
        }) => {
            // The callee value itself is never a linear by-value move (it is a
            // function/global reference), but guard defensively for a Local.
            if let MirValue::Local(l) = callee {
                if is_linear_local(func, *l) {
                    apply_move(func, *l, state, span, errors.as_deref_mut());
                }
            }
            for a in args {
                if let MirValue::Local(l) = a {
                    if is_linear_local(func, *l) {
                        apply_move(func, *l, state, span, errors.as_deref_mut());
                    }
                }
            }
            // The call's dest (re)binds a fresh value.
            if let Some(d) = dest {
                if is_linear_local(func, *d) {
                    state.insert(*d, MoveState::Init);
                }
            }
        }
        Some(MirTerminator::Return(Some(MirValue::Local(l)))) => {
            if is_linear_local(func, *l) {
                apply_move(func, *l, state, span, errors.as_deref_mut());
            }
        }
        Some(MirTerminator::If {
            cond: MirValue::Local(l),
            ..
        })
        | Some(MirTerminator::Switch {
            value: MirValue::Local(l),
            ..
        })
        | Some(MirTerminator::Assert {
            cond: MirValue::Local(l),
            ..
        }) => {
            // A condition READS its operand: borrow-after-move if moved.
            if is_linear_local(func, *l) && state.get(l) == Some(&MoveState::Moved) {
                if let Some(errs) = errors.as_deref_mut() {
                    errs.push(TypeErrorWithSpan::new(
                        TypeError::LinearBorrowAfterMove {
                            name: local_name(func, *l),
                        },
                        span,
                    ));
                }
            }
        }
        _ => {}
    }
}

/// Apply a single MOVE of linear `l`: if `l` is already `Moved`, emit
/// use-after-move; then set `l -> Moved`.
fn apply_move(
    func: &MirFunction,
    l: LocalId,
    state: &mut BlockState,
    span: Span,
    errors: Option<&mut Vec<TypeErrorWithSpan>>,
) {
    if state.get(&l) == Some(&MoveState::Moved) {
        if let Some(errs) = errors {
            errs.push(TypeErrorWithSpan::new(
                TypeError::LinearUseAfterMove {
                    name: local_name(func, l),
                },
                span,
            ));
        }
    }
    state.insert(l, MoveState::Moved);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{
        BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirRValue, MirStmt,
        MirTerminator, MirType, MirValue,
    };
    use std::sync::Arc;

    /// A `#[linear]` local of struct type `Qubit`.
    fn linear_local(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::Struct(Arc::from("Qubit")),
            is_mut: false,
            is_param: false,
            annotations: vec![Arc::from("linear")],
        }
    }

    /// A plain (non-linear) i64 local.
    fn i64_local(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    /// An UNTAGGED `&Qubit` pointer local: type `Ptr(Struct("Qubit"))`, EMPTY
    /// annotations, exactly as real 2a lowering produces for a `&coin` binding
    /// (2a tags only linear-ADT `Struct` locals, never a reference-to-linear).
    /// The move-out-of-shared-borrow detector must recover the borrow from
    /// provenance (`r = &coin`), not from any marker on `r`.
    fn untagged_ptr_local(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::Ptr(Box::new(MirType::Struct(Arc::from("Qubit")))),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    fn count(errors: &[TypeErrorWithSpan], pred: impl Fn(&TypeError) -> bool) -> usize {
        errors.iter().filter(|e| pred(&e.error)).count()
    }

    // 1. Linear moved twice in a single straight-line block -> exactly one
    //    LinearUseAfterMove (the second move; the first is legal).
    //    bb0: _1 = Use(_0) ; _2 = Use(_0) ; return
    #[test]
    fn double_move_reports_use_after_move() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "q"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(2, "_2"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearUseAfterMove { .. }
            )),
            1,
            "exactly one use-after-move (the second consume of `q`): {errors:?}"
        );
        assert_eq!(errors.len(), 1, "no other diagnostics: {errors:?}");
    }

    // 2. Linear moved, then borrowed (`Ref`) -> LinearBorrowAfterMove.
    //    bb0: _1 = Use(_0) ; _2 = Ref(_0) ; return
    #[test]
    fn move_then_borrow_reports_borrow_after_move() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "q"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(2, "_2"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Ref {
                is_mut: false,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearBorrowAfterMove { .. }
            )),
            1,
            "the post-move borrow must be flagged: {errors:?}"
        );
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearUseAfterMove { .. }
            )),
            0,
            "the borrow is not a second move: {errors:?}"
        );
    }

    // 3. Linear moved out of `*r` where `r` is a REAL shared `&Qubit` (empty
    //    annotations, as 2a lowering produces) -> LinearMoveOutOfBorrow. The
    //    detector keys off the borrow provenance `r = &coin`, not a marker on
    //    `r` (which real lowering never stamps).
    //    bb0: _1 = &_0 (shared) ; _2 = Deref(_1) ; return   (_0 linear)
    #[test]
    fn move_out_of_shared_borrow_reports() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "coin"));
        // `r`: a real `&Qubit` binding -> Ptr(Struct), NO "linear" annotation.
        func.locals.push(untagged_ptr_local(1, "r"));
        func.locals.push(i64_local(2, "_2"));
        let mut b0 = MirBlock::new(BlockId(0));
        // r = &coin  (shared borrow of a linear local -> provenance set)
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: false,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        // _2 = *r  (move out of the shared borrow)
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(1)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "moving `*r` out of a real shared borrow must be flagged: {errors:?}"
        );
    }

    // 4. Linear moved ONCE -> no errors.
    //    bb0: _1 = Use(_0) ; return
    #[test]
    fn single_move_is_clean() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "q"));
        func.locals.push(i64_local(1, "_1"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert!(errors.is_empty(), "a single move is legal: {errors:?}");
    }

    // 5. Linear moved, then re-bound, then used again -> no errors (the redef
    //    resets to Init, so the second use consumes a fresh value).
    //    bb0: _1 = Use(_0) ; _0 = Call(make) ; _2 = Use(_0) ; return
    #[test]
    fn move_then_rebind_then_use_is_clean() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "q"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(2, "_2"));
        let mut b0 = MirBlock::new(BlockId(0));
        // move _0 out
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        // re-bind _0 via a call dest (fresh value)
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("make_qubit")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        // use the fresh _0 once
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b1.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1]);

        let errors = check(&func);
        assert!(
            errors.is_empty(),
            "re-binding resets move state; the later use is legal: {errors:?}"
        );
    }

    // 6. Branch: one arm moves _0, both arms join, then _0 moved after the join
    //    -> LinearUseAfterMove (maybe-moved join treats it as moved).
    //    bb0: if cond -> bb1 else bb2
    //    bb1: _1 = Use(_0) -> bb3       (moves on this arm)
    //    bb2: -> bb3                    (does not move)
    //    bb3: _2 = Use(_0) ; return     (move after join)
    #[test]
    fn maybe_moved_join_reports_use_after_move() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "q"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(2, "_2"));
        func.locals.push(i64_local(9, "cond"));

        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b1.terminator = Some(MirTerminator::Goto(BlockId(3)));
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Goto(BlockId(3)));
        let mut b3 = MirBlock::new(BlockId(3));
        b3.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b3.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2, b3]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearUseAfterMove { .. }
            )),
            1,
            "a value maybe-moved on one arm is moved after the join: {errors:?}"
        );
    }

    // 7. Loop (back-edge) that re-binds _0 each iteration and moves it once per
    //    iteration -> no errors. The redef at the loop head resets state before
    //    the per-iteration move.
    //    bb0: _0 = Call(make) -> bb1
    //    bb1(header): if cond -> bb2 else bb3
    //    bb2(body): _1 = Use(_0) ; _0 = Call(make) -> bb1    (move then rebind)
    //    bb3: return
    #[test]
    fn loop_rebind_and_move_each_iteration_is_clean() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "q"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(9, "cond"));

        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("make_qubit")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(2),
            else_block: BlockId(3),
        });
        let mut b2 = MirBlock::new(BlockId(2));
        // move _0 out
        b2.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        // rebind _0 (fresh) and loop back
        b2.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("make_qubit")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2, b3]);

        let errors = check(&func);
        assert!(
            errors.is_empty(),
            "each iteration rebinds then moves once; no double-consume: {errors:?}"
        );
    }

    // 8. A NON-linear local moved twice -> no errors (only linear locals are
    //    checked).
    //    bb0: _1 = Use(_0) ; _2 = Use(_0) ; return   (_0 is NOT linear)
    #[test]
    fn non_linear_double_move_is_ignored() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(0, "x")); // NOT linear
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(2, "_2"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert!(
            errors.is_empty(),
            "non-linear locals are not move-checked: {errors:?}"
        );
    }

    // 9. Linear moved BY VALUE into a store statement (`FieldAssign`), then used
    //    again -> exactly one LinearUseAfterMove. The store consumes the linear,
    //    so the later `Use` is a double-consume, NOT a borrow-after-move.
    //    bb0: _1.field = Use(_0) ; _2 = Use(_0) ; return   (_0 linear)
    #[test]
    fn store_by_value_move_is_consumed() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "coin"));
        func.locals.push(i64_local(1, "obj"));
        func.locals.push(i64_local(2, "_2"));
        let mut b0 = MirBlock::new(BlockId(0));
        // obj.field = coin  (FieldAssign; value = Use(coin): consumes `coin`)
        b0.stmts.push(MirStmt::new(MirStmtKind::FieldAssign {
            base: LocalId(1),
            field_name: Arc::from("field"),
            value: MirRValue::Use(MirValue::Local(LocalId(0))),
        }));
        // _2 = Use(coin)  (coin already Moved -> use-after-move)
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearUseAfterMove { .. }
            )),
            1,
            "the store consumes `coin`; the later use is a use-after-move: {errors:?}"
        );
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearBorrowAfterMove { .. }
            )),
            0,
            "the store is a by-value move, not a borrow-after-move: {errors:?}"
        );
        assert_eq!(errors.len(), 1, "no other diagnostics: {errors:?}");
    }

    // 10. Negative: a `&mut` borrow of a linear, then a deref-move, must NOT emit
    //     LinearMoveOutOfBorrow. Mutable-borrow move-out is a different (allowed
    //     for now) case; only SHARED borrows are the categorical violation.
    //     bb0: r = &mut coin ; a = Deref(r) ; return
    #[test]
    fn move_out_of_mut_borrow_is_not_flagged() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "coin"));
        func.locals.push(untagged_ptr_local(1, "r"));
        func.locals.push(i64_local(2, "a"));
        let mut b0 = MirBlock::new(BlockId(0));
        // r = &mut coin  (mutable borrow: not the shared move-out shape)
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: true,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        // a = *r
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(1)),
                pointee_ty: MirType::Struct(Arc::from("Coin")),
            },
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            0,
            "a `&mut` deref-move is not a shared move-out-of-borrow: {errors:?}"
        );
    }
}
