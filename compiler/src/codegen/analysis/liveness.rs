use std::collections::HashSet;

use crate::codegen::ir::{LocalId, MirFunction, MirRValue, MirStmtKind, MirTerminator, MirValue};

use super::cfg::{
    block_id_index, move_source_chain, stmt_uses_local, terminator_successors,
    terminator_uses_local,
};

/// Per-block liveness. `live_in[i]` / `live_out[i]` are indexed by block position
/// (the same index space `cfg::terminator_successors` returns), NOT by `BlockId`.
pub(crate) struct Liveness {
    pub live_in: Vec<HashSet<LocalId>>,
    pub live_out: Vec<HashSet<LocalId>>,
}

/// Standard backward dataflow to a fixpoint:
///   live_out[b] = union over successors s of live_in[s]
///   live_in[b]  = walk b backward from live_out[b], removing defs and adding uses
/// A "def" is an `Assign { dest }` statement or a `Call { dest: Some }` terminator
/// (the only forms that bind a fresh value to a local; FieldAssign/DerefAssign
/// mutate existing contents and StorageLive/Dead are never emitted). Uses come
/// from the exhaustive `stmt_uses_local`/`terminator_uses_local` queries.
pub(crate) fn compute(func: &MirFunction) -> Liveness {
    let blocks = match &func.blocks {
        Some(b) => b.as_slice(),
        None => {
            return Liveness {
                live_in: Vec::new(),
                live_out: Vec::new(),
            }
        }
    };
    let n = blocks.len();
    let id_to_index = block_id_index(blocks);
    let all_locals: Vec<LocalId> = func.locals.iter().map(|l| l.id).collect();

    let mut live_in: Vec<HashSet<LocalId>> = vec![HashSet::new(); n];
    let mut live_out: Vec<HashSet<LocalId>> = vec![HashSet::new(); n];

    let mut changed = true;
    while changed {
        changed = false;
        // Reverse index order converges quickly for the reducible CFGs the
        // lowering emits; the fixpoint is order-independent for correctness.
        for i in (0..n).rev() {
            // live_out[i] = union of successors' live_in
            let mut new_out: HashSet<LocalId> = HashSet::new();
            for s in terminator_successors(&blocks[i].terminator, &id_to_index) {
                if let Some(sin) = live_in.get(s) {
                    for l in sin {
                        new_out.insert(*l);
                    }
                }
            }

            // Walk the block backward: terminator runs last, then statements.
            let mut live = new_out.clone();
            // Terminator: kill its def, then gen its uses.
            if let Some(MirTerminator::Call { dest: Some(d), .. }) = &blocks[i].terminator {
                live.remove(d);
            }
            for x in &all_locals {
                if terminator_uses_local(&blocks[i].terminator, *x) {
                    live.insert(*x);
                }
            }
            // Statements, last to first: kill def, then gen uses (so `x = x + 1`
            // keeps `x` live-before because the use is re-added after the kill).
            for stmt in blocks[i].stmts.iter().rev() {
                if let MirStmtKind::Assign { dest, .. } = &stmt.kind {
                    live.remove(dest);
                }
                for x in &all_locals {
                    if stmt_uses_local(&stmt.kind, *x) {
                        live.insert(*x);
                    }
                }
            }

            if new_out != live_out[i] {
                live_out[i] = new_out;
                changed = true;
            }
            if live != live_in[i] {
                live_in[i] = live;
                changed = true;
            }
        }
    }

    Liveness { live_in, live_out }
}

/// Locals that alias `owner`'s heap buffer: `owner`, its move sources, and every
/// one-hop borrow temp `T = <alias>.ptr`. Move sources are moved-from (dead after
/// the move) so only their borrows can still point into the buffer; including them
/// is conservative-correct.
fn buffer_aliases(func: &MirFunction, owner: LocalId) -> Vec<LocalId> {
    let blocks = func.blocks.as_deref().unwrap_or(&[]);
    let mut aliases = vec![owner];
    aliases.extend(move_source_chain(owner, blocks));
    let mut borrows = Vec::new();
    for block in blocks {
        for stmt in &block.stmts {
            if let MirStmtKind::Assign {
                dest,
                value:
                    MirRValue::FieldAccess {
                        base: MirValue::Local(b),
                        ..
                    },
            } = &stmt.kind
            {
                if aliases.contains(b) {
                    borrows.push(*dest);
                }
            }
        }
    }
    aliases.extend(borrows);
    aliases
}

fn buffer_live(sets: &[HashSet<LocalId>], aliases: &[LocalId]) -> Vec<bool> {
    sets.iter()
        .map(|s| aliases.iter().any(|a| s.contains(a)))
        .collect()
}

/// Whether `owner`'s buffer is live at each block's ENTRY (indexed by block position).
pub(crate) fn buffer_live_in(func: &MirFunction, live: &Liveness, owner: LocalId) -> Vec<bool> {
    buffer_live(&live.live_in, &buffer_aliases(func, owner))
}

/// Whether `owner`'s buffer is live at each block's EXIT (indexed by block position).
pub(crate) fn buffer_live_out(func: &MirFunction, live: &Liveness, owner: LocalId) -> Vec<bool> {
    buffer_live(&live.live_out, &buffer_aliases(func, owner))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{
        BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirRValue, MirStmt, MirTerminator,
        MirType, MirValue,
    };
    use std::sync::Arc;

    fn i64_local(id: u32, name: &str) -> crate::codegen::ir::MirLocal {
        crate::codegen::ir::MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    // bb0: _0 = call f() -> bb1 ; bb1: _1 = Use(_0) ; return _1
    // _0 is the cross-block live range (def in bb0 term, moved in bb1); _1 is
    // defined and consumed entirely inside bb1.
    #[test]
    fn straight_line_live_ranges() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("f")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b1.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        func.blocks = Some(vec![b0, b1]);

        let live = compute(&func);
        // _0 flows out of bb0 into bb1 (used by the move in bb1): it is the
        // cross-block live range, so it appears in both boundary sets.
        assert!(live.live_out[0].contains(&LocalId(0)));
        assert!(live.live_in[1].contains(&LocalId(0)));
        // _0 is dead after the move: bb1 consumes it, so it is not carried out.
        assert!(!live.live_out[1].contains(&LocalId(0)));
        // _1 is defined and consumed inside bb1 (`_1 = Use(_0); return _1`), so
        // its whole live range is intra-block: it is live at the return but is
        // killed by its own def before block entry, hence absent from both
        // boundary sets of a per-block model. live_out[1] is empty because
        // Return has no successors.
        assert!(live.live_out[1].is_empty());
        assert!(!live.live_in[1].contains(&LocalId(1)));
    }

    // A back-edge loop: a local defined and used only inside the body is NOT
    // live across the back-edge (dead at the header entry from the body).
    #[test]
    fn loop_body_local_dead_across_back_edge() {
        // bb0 -> bb1(header) ; bb1: if cond -> bb2 else bb3
        // bb2(body): _2 = call g(); _3 = Use(_2); (use _3) -> bb1
        // bb3: return
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(0, "cond"));
        func.locals.push(i64_local(2, "_2"));
        func.locals.push(i64_local(3, "_3"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Goto(BlockId(1)));
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(0)),
            then_block: BlockId(2),
            else_block: BlockId(3),
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("g")),
            args: vec![MirValue::Local(LocalId(3))], // uses _3 then loops
            dest: Some(LocalId(2)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2, b3]);

        let live = compute(&func);
        // _2 is defined at the end of bb2 (call dest) and used nowhere else, so it
        // is not live out of bb2 and not live into the header from the body.
        assert!(!live.live_out[2].contains(&LocalId(2)));
        assert!(!live.live_in[1].contains(&LocalId(2)));
    }

    // bb0: _0 = call alloc() -> bb1
    // bb1: _1 = _0.ptr ; call print(_1) -> bb2      (borrow of the buffer, used here)
    // bb2: return
    // The buffer is live in bb0(out) and bb1, dead at bb2 entry.
    #[test]
    fn buffer_stays_live_through_ptr_borrow() {
        use crate::codegen::ir::MirLocal;
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(MirLocal {
            id: LocalId(0),
            name: Some(Arc::from("_0")),
            ty: MirType::Struct(Arc::from("BuildString")),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        });
        func.locals.push(i64_local(1, "_1")); // the .ptr borrow temp
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: None,
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2]);

        let live = compute(&func);
        let bin = buffer_live_in(&func, &live, LocalId(0));
        let bout = buffer_live_out(&func, &live, LocalId(0));
        // Buffer live leaving bb0 and entering bb1 (via the .ptr borrow), dead at bb2.
        assert!(bout[0], "buffer live out of bb0");
        assert!(bin[1], "buffer live into bb1 via _0 or its .ptr borrow");
        assert!(
            !bin[2],
            "buffer dead at bb2 entry (borrow consumed by bb1 printf)"
        );
    }
}
