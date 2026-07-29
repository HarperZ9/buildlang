//! Increment 5: split-frontier drop flags. Reclaims owners whose death
//! frontier is SPLIT across conditional edges (used on one arm of an `if`,
//! not the other) and owners whose ALLOCATION is conditional (def block does
//! not dominate the frees) -- the two shapes increments 1-4 decline, most
//! directly witnessed by `drops.rs`'s `declines_split_death_frontier` unit
//! test. Mechanism: a per-buffer runtime `uint8_t` drop flag emitted by the C
//! backend (`backend::c`), tested and cleared at every free. Additive and
//! disjoint from increments 1-4 (`fn_exit`/`block_scoped`/`multi_block`): an
//! owner already claimed by any of those is skipped here.
//!
//! # The soundness invariant (verbatim; also carried in `backend::c`'s
//! flag-emission code, per the design doc)
//!
//! At every point in the emitted C, `__bl_live_N == 1` implies `L` currently
//! holds a fresh allocated buffer that no free site has released. Established
//! by: init 0 at declaration; set 1 ONLY immediately after `L`'s unique def
//! (an allocating call or a move-acquire of a fresh buffer whose source is
//! moved-from and therefore excluded from every free set); cleared to 0
//! immediately after every guarded free; `build_string_free` reached only
//! under the flag test. The flag guards ALLOCATED (double free, uninitialized
//! free). It does not guard LIVENESS: a site must additionally satisfy
//! buffer-dead-at-entry (no future use of the owner or any borrow temp on any
//! path), which is what forbids use-after-free. Both halves are load-bearing.
//!
//! # Soundness precondition (load-bearing, do not relax)
//!
//! `candidates` MUST be escape-filtered (e.g. from `CBackend::sound_owned_candidates`);
//! this function trusts them and does not re-verify non-escape, because the
//! buffer-liveness overlay is blind to multi-hop `.ptr` copies and
//! `Ref`/`AddressOf` borrows. Passing an unfiltered owner would be unsound.
//! In production `candidates` come exclusively from `CBackend::sound_owned_candidates`,
//! which runs `owned_string_escapes` (the conservative gate that rejects
//! multi-hop `.ptr` copies and `Ref`/`AddressOf` aliasing) before this
//! function ever sees them.

use std::collections::{HashMap, HashSet};

use crate::codegen::ir::{LocalId, MirFunction};

use super::cfg::{block_id_index, compute_dominators, reachable_blocks, terminator_successors};
use super::liveness::{self, buffer_live_in, buffer_live_out};

/// Flag-managed owners and their guarded block-start free sites.
pub(crate) struct FlagFrees {
    /// Owners enrolled for flag management, sorted by `LocalId`. Drives the
    /// flag declaration, the def-site set, and the Return backstop frees (see
    /// `backend::c`: every enrolled owner gets a guarded free at every
    /// `Return`, in addition to any block-start sites below).
    pub owners: Vec<LocalId>,
    /// Guarded frees at block START, keyed by the C `bb<id>` (`BlockId.0`),
    /// values sorted by `LocalId`. Same key space as increment 3/4's map.
    pub block_frees: HashMap<u32, Vec<LocalId>>,
}

/// Placement rule for split-frontier / conditional-allocation drop flags.
///
/// For each owner `(L, def_bi)` in `candidates` NOT already in `claimed`
/// (the union of every prior increment's claimed-owner sets: `fn_exit`,
/// `block_scoped`, `multi_block_freeable`'s output -- disjointness, the
/// no-double-free half):
///
/// 1. Enroll `L` as a flag owner (unconditionally: even an owner whose only
///    death frontier turns out to be re-entrant, see step 3, still gets a
///    flag, because the Return backstop alone remains sound and reclaims it
///    on every path that exits the function).
/// 2. Compute per-block buffer liveness (`buffer_live_in`/`buffer_live_out`
///    over the same overlay increment 4 trusts, which already folds in `L`'s
///    move-source chain and one-hop `.ptr` borrow temps) and
///    `terminal[b] = buf_in[b] && !buf_out[b]` (a real death happened
///    somewhere inside `b`).
/// 3. A block `S` is a frontier free site iff:
///    - `S` is reachable, not the entry block, and has at least one
///      predecessor;
///    - `!buf_in[S]` (buffer dead at `S`'s entry -- the UAF guard: no future
///      use of the owner or any borrow temp on any path from `S`);
///    - some predecessor `p` of `S` has `terminal[p]` (a real death happened
///      just upstream, so there is something to free; this is also what
///      keeps the site set small, mirroring `drops.rs`: blocks after a death
///      are clean, so no downstream block re-qualifies);
///    - `S` is NOT re-entrant: no predecessor `p` of `S` has `S` in `dom[p]`
///      (a back-edge into `S`, i.e. a loop header or self-loop). `S`'s START
///      would otherwise re-execute once per iteration; freeing a
///      once-allocated buffer there would double-free on every subsequent
///      iteration. Kept verbatim from increment 4's exclusion: the flag
///      would make a re-entrant site sound (the guard blocks the second
///      free), but relaxing this is a named follow-up, not this brick.
///    Unlike increment 4 there is NO dominance requirement (`def_bi` need
///    not dominate `S`: the flag covers conditional allocation) and NO
///    uniqueness requirement (multiple sites on different paths are fine;
///    the clear makes a later site on the same path a no-op).
/// 4. The Return backstop (every enrolled owner freed, guarded, at every
///    `Return`) is emitted by `backend::c` directly from `owners`, not
///    tracked here: it reclaims paths that bypass every frontier site (early
///    return, break out of a loop mid-iteration) and is sound
///    unconditionally, since nothing executes after the block's statements
///    at a `Return` terminator and non-escape means no pointer derived from
///    `L` survives the function.
pub(crate) fn split_frontier_flag_frees(
    func: &MirFunction,
    candidates: &[(LocalId, usize)],
    claimed: &HashSet<LocalId>,
) -> FlagFrees {
    let mut owners: Vec<LocalId> = Vec::new();
    let mut block_frees: HashMap<u32, Vec<LocalId>> = HashMap::new();

    let blocks = match &func.blocks {
        Some(b) if !b.is_empty() => b.as_slice(),
        _ => {
            return FlagFrees {
                owners,
                block_frees,
            }
        }
    };
    let n = blocks.len();
    let id_to_index = block_id_index(blocks);
    let entry = id_to_index.get(&0).copied().unwrap_or(0);
    let reachable = reachable_blocks(blocks);
    let dom = compute_dominators(blocks);

    // Predecessor lists over reachable blocks only (mirrors drops.rs).
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

    for &(owner, _def_bi) in candidates {
        if claimed.contains(&owner) {
            continue; // disjointness: already freed by a prior increment
        }
        owners.push(owner);

        let live = liveness::compute(func);
        let buf_in = buffer_live_in(func, &live, owner);
        let buf_out = buffer_live_out(func, &live, owner);
        // terminal[b]: the buffer is live somewhere inside b and is fully
        // consumed by b's exit (a real use/death happened in this block).
        let terminal: Vec<bool> = (0..n).map(|b| buf_in[b] && !buf_out[b]).collect();

        for s in 0..n {
            if !reachable[s] || s == entry || preds[s].is_empty() {
                continue;
            }
            if buf_in[s] {
                continue; // buffer still live at S's entry: not a death frontier (UAF guard)
            }
            if !preds[s].iter().any(|&p| terminal[p]) {
                continue; // no real death happened anywhere upstream: nothing to free
            }
            // Re-entrant exclusion: a predecessor p with S dominating p is a
            // back-edge p -> S (S is a loop header, or S == p is a self-loop,
            // since dom[p] always contains p).
            if preds[s].iter().any(|&p| dom[p].contains(&s)) {
                continue;
            }
            block_frees.entry(blocks[s].id.0).or_default().push(owner);
        }
    }

    owners.sort_by_key(|id| id.0);
    for v in block_frees.values_mut() {
        v.sort_by_key(|id| id.0);
    }
    FlagFrees {
        owners,
        block_frees,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{
        BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirRValue, MirStmt,
        MirTerminator, MirType, MirValue,
    };
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

    // The exact CFG `drops.rs::declines_split_death_frontier` declines:
    // bb0: _0 = alloc() -> bb0b (BlockId 4, since a Call terminator cannot
    //      also branch)
    // bb0b: if cond -> bb1 else bb2
    // bb1: _1 = _0.ptr ; print(_1) -> bb3   (buffer used on this path)
    // bb2: -> bb3                            (buffer unused on this path)
    // bb3: return                            (join; the split frontier)
    fn split_frontier_func() -> MirFunction {
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
        func
    }

    #[test]
    fn enrolls_split_frontier_and_places_join_site() {
        let func = split_frontier_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let claimed = HashSet::new();
        let frees = split_frontier_flag_frees(&func, &candidates, &claimed);
        assert_eq!(
            frees.owners,
            vec![LocalId(0)],
            "the split-frontier owner must be enrolled: {:?}",
            frees.owners
        );
        assert_eq!(
            frees.block_frees.get(&3).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "exactly one guarded site at the join bb3: {:?}",
            frees.block_frees
        );
        assert_eq!(
            frees.block_frees.len(),
            1,
            "no site anywhere else: {:?}",
            frees.block_frees
        );

        // The pair that proves "4-declines/5-claims": on this exact CFG,
        // increment 4's multi_block_freeable still declines (it has no flag
        // to guard the uninitialized free on the bb2 path).
        let extra = super::super::drops::multi_block_freeable(
            &func,
            &candidates,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert!(
            extra.is_empty(),
            "increment 4 must still decline this split frontier: {extra:?}"
        );
    }

    // Allocation inside ONE arm of an `if`, used in that arm, join then
    // return. def_bi (bb1) does NOT dominate the join (bb3): the bb2 arm
    // reaches bb3 without ever passing through bb1. No dominance requirement
    // in the flag placement rule (unlike increment 4), so the owner is
    // enrolled and gets a site at the join.
    // bb0: if cond -> bb1 else bb2
    // bb1: _0 = alloc() -> bb1b
    // bb1b: _1 = _0.ptr ; print(_1) -> bb3
    // bb2: -> bb3
    // bb3: return
    fn conditional_alloc_func() -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(9, "cond"));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));

        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        });

        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(4)),
            unwind: None,
        });

        let mut b1b = MirBlock::new(BlockId(4));
        b1b.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1b.terminator = Some(MirTerminator::Call {
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

        func.blocks = Some(vec![b0, b1, b1b, b2, b3]);
        func
    }

    #[test]
    fn conditional_alloc_enrolls_with_no_dominance() {
        let func = conditional_alloc_func();
        // def_bi is bb1's index (1 in the blocks vec).
        let candidates = vec![(LocalId(0), 1usize)];
        let dom = compute_dominators(func.blocks.as_ref().unwrap());
        let id_to_index = block_id_index(func.blocks.as_ref().unwrap());
        let join_idx = id_to_index[&3];
        let def_idx = 1usize;
        assert!(
            !dom[join_idx].contains(&def_idx),
            "precondition: def block must NOT dominate the join, or this test proves nothing"
        );

        let claimed = HashSet::new();
        let frees = split_frontier_flag_frees(&func, &candidates, &claimed);
        assert_eq!(frees.owners, vec![LocalId(0)], "owner must be enrolled");
        assert_eq!(
            frees.block_frees.get(&3).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "site at the join despite non-dominating def: {:?}",
            frees.block_frees
        );
    }

    // The split-frontier CFG extended with a SECOND real use of the buffer at
    // the join itself (bb3), then a tail block bb5 before return. buf_in[bb3]
    // is therefore true (bb3 uses the buffer), so bb3 must NOT be a site; the
    // only site is bb5, past the join's own last use.
    fn live_join_func() -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(9, "cond"));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(2, "_2"));

        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(4)),
            unwind: None,
        });

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

        // The join itself uses the buffer again: buf_in[bb3] is true.
        let mut b3 = MirBlock::new(BlockId(3));
        b3.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b3.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(2))],
            dest: None,
            target: Some(BlockId(5)),
            unwind: None,
        });

        let mut b5 = MirBlock::new(BlockId(5));
        b5.terminator = Some(MirTerminator::Return(None));

        func.blocks = Some(vec![b0, b0b, b1, b2, b3, b5]);
        func
    }

    #[test]
    fn uaf_shape_declines_live_join() {
        let func = live_join_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let claimed = HashSet::new();
        let frees = split_frontier_flag_frees(&func, &candidates, &claimed);
        assert!(
            frees.owners.contains(&LocalId(0)),
            "owner is still a candidate (claimed is empty): {:?}",
            frees.owners
        );
        assert!(
            !frees.block_frees.contains_key(&3),
            "the join bb3 still USES the buffer (buf_in true): must not be a site: {:?}",
            frees.block_frees
        );
        assert_eq!(
            frees.block_frees.get(&5).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "the only site is past the join's own last use, at bb5: {:?}",
            frees.block_frees
        );
    }

    // Mirrors drops.rs's declines_loop_header_death_block and
    // declines_self_loop_death_block: a back-edge predecessor makes S
    // re-entrant, so S must never be a site, even though it otherwise
    // qualifies (buffer dead at entry, a terminal predecessor exists).
    #[test]
    fn declines_reentrant_site() {
        // Loop header shape:
        // bb0: _0 = alloc() -> bb1
        // bb1: _1 = _0.ptr ; printf(_1) -> bb2     (terminal: buffer dies here)
        // bb2 (header): if cond -> bb3 else bb4
        // bb3 (body): Goto bb2                       (back-edge bb3 -> bb2)
        // bb4: return
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

        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(3),
            else_block: BlockId(4),
        });

        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Goto(BlockId(2)));

        let mut b4 = MirBlock::new(BlockId(4));
        b4.terminator = Some(MirTerminator::Return(None));

        func.blocks = Some(vec![b0, b1, b2, b3, b4]);

        let candidates = vec![(LocalId(0), 0usize)];
        let claimed = HashSet::new();
        let frees = split_frontier_flag_frees(&func, &candidates, &claimed);
        assert!(
            frees.owners.contains(&LocalId(0)),
            "owner still enrolled: the Return backstop alone reclaims it: {:?}",
            frees.owners
        );
        assert!(
            frees.block_frees.is_empty(),
            "the only candidate site (bb2, the loop header) is re-entrant: must decline: {:?}",
            frees.block_frees
        );

        // Self-loop shape: bb2 branches to itself.
        // bb0: _0 = alloc() -> bb1
        // bb1: _1 = _0.ptr ; printf(_1) -> bb2
        // bb2: if cond -> bb2 else bb3   (self-loop back-edge bb2 -> bb2)
        // bb3: return
        let mut func2 = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func2.locals.push(bs(0, "_0"));
        func2.locals.push(i64_local(1, "_1"));
        func2.locals.push(i64_local(9, "cond"));

        let mut c0 = MirBlock::new(BlockId(0));
        c0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut c1 = MirBlock::new(BlockId(1));
        c1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        c1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: None,
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut c2 = MirBlock::new(BlockId(2));
        c2.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(2),
            else_block: BlockId(3),
        });
        let mut c3 = MirBlock::new(BlockId(3));
        c3.terminator = Some(MirTerminator::Return(None));
        func2.blocks = Some(vec![c0, c1, c2, c3]);

        let frees2 = split_frontier_flag_frees(&func2, &candidates, &claimed);
        assert!(
            frees2.owners.contains(&LocalId(0)),
            "self-loop shape: owner still enrolled: {:?}",
            frees2.owners
        );
        assert!(
            frees2.block_frees.is_empty(),
            "self-loop shape: bb2 is re-entrant (dom[bb2] contains bb2 trivially): must decline: {:?}",
            frees2.block_frees
        );
    }

    #[test]
    fn claimed_owner_not_enrolled() {
        let func = split_frontier_func();
        let candidates = vec![(LocalId(0), 0usize)];
        let mut claimed = HashSet::new();
        claimed.insert(LocalId(0));
        let frees = split_frontier_flag_frees(&func, &candidates, &claimed);
        assert!(
            frees.owners.is_empty(),
            "disjointness: a claimed owner must not be enrolled: {:?}",
            frees.owners
        );
        assert!(
            frees.block_frees.is_empty(),
            "disjointness: a claimed owner gets no sites: {:?}",
            frees.block_frees
        );
    }

    // bb0: _0 = alloc() -> bb1
    // bb1: _1 = _0.ptr ; printf(_1) -> bb2     (terminal: the buffer dies here)
    // bb2: -> bb3                                (clean: first site)
    // bb3: -> bb4                                (clean: must NOT re-qualify)
    // bb4: return                                (clean: must NOT re-qualify)
    #[test]
    fn no_site_without_terminal_pred() {
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
        b2.terminator = Some(MirTerminator::Goto(BlockId(3)));

        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Goto(BlockId(4)));

        let mut b4 = MirBlock::new(BlockId(4));
        b4.terminator = Some(MirTerminator::Return(None));

        func.blocks = Some(vec![b0, b1, b2, b3, b4]);

        let candidates = vec![(LocalId(0), 0usize)];
        let claimed = HashSet::new();
        let frees = split_frontier_flag_frees(&func, &candidates, &claimed);
        assert_eq!(
            frees.block_frees.get(&2).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "bb2 is the one real death frontier (terminal pred bb1): {:?}",
            frees.block_frees
        );
        assert!(
            !frees.block_frees.contains_key(&3) && !frees.block_frees.contains_key(&4),
            "clean blocks downstream of a non-terminal predecessor must not re-qualify: {:?}",
            frees.block_frees
        );
        assert_eq!(
            frees.block_frees.len(),
            1,
            "exactly one site total: {:?}",
            frees.block_frees
        );
    }
}
