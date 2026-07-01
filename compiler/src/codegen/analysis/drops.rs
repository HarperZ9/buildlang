//! Increment 4: multi-block drop placement. Frees an owner whose live range
//! spans multiple blocks at the unique block where its buffer dies on every
//! incoming edge, using the liveness + buffer-liveness substrate from
//! `super::liveness`. Additive and disjoint from increments 1-3
//! (`fn_exit`/`block_scoped`).

use std::collections::{HashMap, HashSet};

use crate::codegen::ir::{LocalId, MirFunction};

use super::cfg::{block_id_index, compute_dominators, reachable_blocks, terminator_successors};
use super::liveness::{self, buffer_live_in, buffer_live_out};

/// Additional block-start frees for owners whose live range spans multiple blocks
/// (increment 4). Disjoint from `fn_exit` (increments 1-2) and `block_scoped`
/// (increment 3): an owner in either is skipped, so each buffer is freed once.
///
/// # Soundness precondition (load-bearing, do not relax)
///
/// `candidates` MUST be escape-filtered (e.g. from `sound_owned_candidates`);
/// this function trusts them and does not re-verify non-escape, because the
/// buffer-liveness overlay is blind to multi-hop `.ptr` copies and
/// `Ref`/`AddressOf` borrows. Passing an unfiltered owner would be unsound.
/// In production `candidates` come exclusively from `CBackend::sound_owned_candidates`,
/// which runs `owned_string_escapes` (the conservative gate that rejects
/// multi-hop `.ptr` copies and `Ref`/`AddressOf` aliasing) before this function
/// ever sees them.
///
/// Sound-conservative "single clean death frontier" rule. Free owner `L` at the
/// START of block `S` iff there is EXACTLY ONE reachable, non-entry block `S`
/// such that:
///   1. `L`'s buffer is DEAD at `S`'s entry, and
///   2. every predecessor `P` of `S` is either a TERMINAL block (the buffer is
///      live somewhere in `P` and dies by `P`'s exit: `buf_in[P]` true, `buf_out[P]`
///      false — a real use/consumption happened in `P`) or a CLEAN block (the
///      buffer is dead both at `P`'s entry and exit: never live in `P` at all),
///      with AT LEAST ONE terminal predecessor, and every clean predecessor's own
///      predecessors recursively satisfy the same terminal-or-clean property back
///      to `L`'s def block (a clean block that descends from a block where the
///      buffer WAS live is a branch that skipped the buffer's only use — the
///      signature of a split frontier — and voids this `S`), and
///   3. `L`'s defining block dominates `S`.
///
/// Block-level `buf_out[P]` is a union over ALL of `P`'s successors, so it cannot
/// distinguish "the buffer dies on the P->S edge specifically" from "P has some
/// OTHER successor that still needs it" (exactly the split-frontier shape: an
/// `if`/`else` where one arm uses the buffer and the other doesn't). Requiring
/// `buf_out[P]` true for every predecessor of `S` is therefore neither necessary
/// (a predecessor that consumes the buffer via a real use inside itself has
/// `buf_out` false, e.g. the block containing the last `.ptr` borrow's consuming
/// call) nor sufficient (a predecessor's `buf_out` can be true purely because of a
/// sibling successor). The terminal/clean walk above uses only per-block facts
/// (`buf_in`/`buf_out`, which are already sound) plus dominance, so it stays a
/// pure function of the same liveness substrate without needing new dataflow.
///
/// `S` is additionally required to be NON-RE-ENTRANT: any block reached via a
/// back-edge (a loop header, or a self-loop block — a predecessor `P` that `S`
/// dominates) is declined, because `S`'s START re-executes once per iteration and
/// freeing a once-allocated buffer there would double-free on every subsequent
/// iteration. With that exclusion the free at `S` runs at most once per allocation.
///
/// Then freeing at `S`'s start runs after every use (buffer dead at entry means no
/// use at/after `S`; every path into `S` passed through a terminal block that
/// consumed the buffer), on every acyclic path reaching `S`, and only when `L` was
/// allocated (def dominates `S`). Zero or >1 such `S`, a re-entrant `S`, or any
/// predecessor chain that fails the terminal/clean property (a split frontier
/// needing a drop flag), declines: the buffer leaks, which is safe.
pub(crate) fn multi_block_freeable(
    func: &MirFunction,
    candidates: &[(LocalId, usize)],
    fn_exit: &HashSet<LocalId>,
    block_scoped: &HashSet<LocalId>,
) -> HashMap<u32, Vec<LocalId>> {
    let mut map: HashMap<u32, Vec<LocalId>> = HashMap::new();
    let blocks = match &func.blocks {
        Some(b) if !b.is_empty() => b.as_slice(),
        _ => return map,
    };
    let n = blocks.len();
    let id_to_index = block_id_index(blocks);
    let entry = id_to_index.get(&0).copied().unwrap_or(0);
    let reachable = reachable_blocks(blocks);
    let dom = compute_dominators(blocks);
    let live = liveness::compute(func);

    // Predecessor lists over reachable blocks only.
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

    for &(owner, def_bi) in candidates {
        if fn_exit.contains(&owner) || block_scoped.contains(&owner) {
            continue; // disjointness
        }
        let buf_in = buffer_live_in(func, &live, owner);
        let buf_out = buffer_live_out(func, &live, owner);
        // terminal[b]: the buffer is live somewhere inside b and is fully
        // consumed by b's exit (a real use/death happened in this block).
        // clean[b]: the buffer was never live in b at all (entry and exit dead).
        let terminal: Vec<bool> = (0..n).map(|b| buf_in[b] && !buf_out[b]).collect();
        let clean: Vec<bool> = (0..n).map(|b| !buf_in[b] && !buf_out[b]).collect();

        // Bounded backward walk: is every predecessor of `b` (transitively,
        // through clean blocks only, stopping at any terminal block) either
        // terminal or clean, with at least one terminal reached on every
        // backward path before falling off dominance of `def_bi`? Memoized
        // per owner since `n` is small and blocks form a DAG modulo the
        // dominance bound (a clean block's predecessors are strictly "earlier"
        // on any path that still reaches `S` under dominance, so this
        // terminates: revisits are cut by the `visited` guard).
        fn clean_chain_ok(
            b: usize,
            preds: &[Vec<usize>],
            terminal: &[bool],
            clean: &[bool],
            def_bi: usize,
            dom: &[HashSet<usize>],
            visited: &mut HashSet<usize>,
        ) -> bool {
            if !visited.insert(b) {
                return true; // already verified on another path
            }
            if b == def_bi {
                // Reached the def block via an all-clean chain with no terminal
                // block in between: the buffer was never used on this path, so
                // there is nothing to free here — not a valid death frontier.
                return false;
            }
            if preds[b].is_empty() {
                return false; // ran off the CFG without a terminal block: invalid
            }
            for &p in &preds[b] {
                if terminal[p] {
                    continue; // this backward path is anchored by a real death
                }
                if clean[p] && dom[b].contains(&def_bi) {
                    if !clean_chain_ok(p, preds, terminal, clean, def_bi, dom, visited) {
                        return false;
                    }
                    continue;
                }
                return false; // neither terminal nor clean-under-dominance: split
            }
            true
        }

        let mut death_blocks: Vec<usize> = Vec::new();
        for s in 0..n {
            if !reachable[s] || s == entry || buf_in[s] || preds[s].is_empty() {
                continue;
            }
            if !dom[s].contains(&def_bi) {
                continue; // def must dominate the free site
            }
            // A loop header (a predecessor reached via a back-edge into S) executes
            // once per iteration; freeing at its START would double-free a
            // once-allocated buffer. Decline; the buffer leaks, which is safe.
            // (Legitimate loop-body frees land on non-header blocks and are
            // unaffected.) S is a loop header iff some predecessor P has a back-edge
            // P -> S, i.e. S dominates P.
            if preds[s].iter().any(|&p| dom[p].contains(&s)) {
                continue;
            }
            if !preds[s].iter().any(|&p| terminal[p]) {
                continue; // no real death anywhere upstream: nothing to free
            }
            let mut visited = HashSet::new();
            let ok = preds[s].iter().all(|&p| {
                terminal[p]
                    || (clean[p]
                        && clean_chain_ok(p, &preds, &terminal, &clean, def_bi, &dom, &mut visited))
            });
            if !ok {
                continue; // split frontier: some predecessor chain skips the use
            }
            death_blocks.push(s);
        }
        if death_blocks.len() != 1 {
            continue; // zero or split -> decline (leak, safe)
        }
        map.entry(blocks[death_blocks[0]].id.0)
            .or_default()
            .push(owner);
    }

    for v in map.values_mut() {
        v.sort_by_key(|id| id.0);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{
        BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirRValue, MirStmt,
        MirTerminator, MirType, MirValue,
    };
    use std::collections::HashSet;
    use std::sync::Arc;

    fn bs(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::Struct(Arc::from("BuildString")),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }
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

    // Multi-block live range increment 3 declines (not single-block confined):
    // bb0: _0 = alloc() -> bb1
    // bb1: _1 = _0.ptr ; print(_1) -> bb2      (owner defined bb0, used bb1)
    // bb2: return
    // Buffer dies at bb2 entry; bb2's only pred bb1 has it live out; bb0 dominates bb2.
    // -> free _0 at start of bb2.
    fn multi_block_owner_func() -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
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
        func
    }

    #[test]
    fn frees_multi_block_owner_at_death_block_start() {
        let func = multi_block_owner_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let map = multi_block_freeable(&func, &candidates, &HashSet::new(), &HashSet::new());
        assert_eq!(
            map.get(&2).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "owner must be freed at the start of bb2 (the buffer's death block): {map:?}"
        );
    }

    #[test]
    fn skips_owner_already_claimed_by_fn_exit() {
        let func = multi_block_owner_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let mut fn_exit = HashSet::new();
        fn_exit.insert(LocalId(0));
        let map = multi_block_freeable(&func, &candidates, &fn_exit, &HashSet::new());
        assert!(
            map.is_empty(),
            "disjointness: fn_exit owner must not be re-freed"
        );
    }

    #[test]
    fn skips_owner_already_claimed_by_block_scoped() {
        let func = multi_block_owner_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let mut bscoped = HashSet::new();
        bscoped.insert(LocalId(0));
        let map = multi_block_freeable(&func, &candidates, &HashSet::new(), &bscoped);
        assert!(
            map.is_empty(),
            "disjointness: block-scoped owner must not be re-freed"
        );
    }

    // Split death frontier: buffer live on one incoming edge of the join, dead on
    // the other -> needs a drop flag -> DECLINE.
    // bb0: _0 = alloc() -> bb0b
    // bb0b: if cond -> bb1 else bb2
    // bb1: _1 = _0.ptr ; print(_1) -> bb3        (buffer used on this path)
    // bb2: -> bb3                                 (buffer unused on this path)
    // bb3: return                                 (join)
    #[test]
    fn declines_split_death_frontier() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(9, "cond"));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));

        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(4)),
            unwind: None,
        });

        // bb0b (BlockId(4)) branches after the alloc: a Call terminator cannot
        // also branch, so the alloc lives in bb0 and the branch lives in bb0b.
        let mut b0b = MirBlock::new(BlockId(4));
        b0b.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(1),
            else_block: BlockId(2),
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
            target: Some(BlockId(3)),
            unwind: None,
        });

        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Goto(BlockId(3)));

        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));

        func.blocks = Some(vec![b0, b0b, b1, b2, b3]);

        // def block index of _0 is 0 (bb0). Candidate offered; rule must DECLINE:
        // bb1 is a TERMINAL predecessor of the join bb3 (consumes the buffer via
        // its .ptr use), but bb2 is a CLEAN predecessor (buffer never live in it)
        // that descends from bb0b where the buffer WAS live (bb0b's other arm,
        // bb1, needed it) — a clean block downstream of a live block is exactly
        // the split-frontier signature, so the backward walk from bb2 fails and
        // the join is declined. bb2 alone (as its own candidate S) is declined
        // too: its only predecessor bb0b is neither terminal nor clean.
        let candidates = vec![(LocalId(0), 0usize)];
        let map = multi_block_freeable(&func, &candidates, &HashSet::new(), &HashSet::new());
        assert!(
            map.is_empty(),
            "split death frontier needs a drop flag; must decline (leak, safe): {map:?}"
        );
    }

    // Loop-header death block: the buffer dies in bb1, and bb2 is a loop header
    // (bb3 is a body block with a back-edge bb3 -> bb2). bb2's block START executes
    // once per iteration, so freeing a once-allocated buffer there would double-free
    // on every subsequent iteration. Rule must DECLINE (leak, safe).
    // bb0: _0 = alloc() -> bb1
    // bb1: _1 = _0.ptr ; printf(_1) -> bb2        (buffer dies in bb1)
    // bb2 (header): if cond -> bb3 else bb4
    // bb3 (body): Goto bb2                          (back-edge bb3 -> bb2)
    // bb4: return
    #[test]
    fn declines_loop_header_death_block() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(9, "cond"));

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

        // bb2 is the loop header: reached from bb1 (entry) and from bb3 (back-edge).
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(3),
            else_block: BlockId(4),
        });

        // bb3 is the loop body; its Goto back to bb2 forms the back-edge.
        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Goto(BlockId(2)));

        let mut b4 = MirBlock::new(BlockId(4));
        b4.terminator = Some(MirTerminator::Return(None));

        func.blocks = Some(vec![b0, b1, b2, b3, b4]);

        let candidates = vec![(LocalId(0), 0usize)];
        let map = multi_block_freeable(&func, &candidates, &HashSet::new(), &HashSet::new());
        assert!(
            map.get(&2).is_none(),
            "bb2 is a loop header; freeing there double-frees per iteration — must decline: {map:?}"
        );
        assert!(
            map.is_empty(),
            "no valid free site for a buffer that dies before a loop header: {map:?}"
        );
    }

    // Self-loop death block: bb2 branches to itself (back-edge bb2 -> bb2), so its
    // START re-executes each iteration. Same double-free hazard as a loop header.
    // bb0: _0 = alloc() -> bb1
    // bb1: _1 = _0.ptr ; printf(_1) -> bb2        (buffer dies in bb1)
    // bb2: if cond -> bb2 else bb3                  (self-loop back-edge bb2 -> bb2)
    // bb3: return
    #[test]
    fn declines_self_loop_death_block() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(9, "cond"));

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

        // bb2 self-loops: reached from bb1 and from itself (back-edge bb2 -> bb2).
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(2),
            else_block: BlockId(3),
        });

        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));

        func.blocks = Some(vec![b0, b1, b2, b3]);

        let candidates = vec![(LocalId(0), 0usize)];
        let map = multi_block_freeable(&func, &candidates, &HashSet::new(), &HashSet::new());
        assert!(
            map.get(&2).is_none(),
            "bb2 self-loops; freeing there double-frees per iteration — must decline: {map:?}"
        );
    }
}
