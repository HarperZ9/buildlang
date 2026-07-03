//! Structured control-flow reconstruction for SPIR-V.
//!
//! SPIR-V is a *structured* IR: every selection header must name a merge block
//! that post-dominates it, and every loop header must name both a merge (exit)
//! block and a continue target. The merge blocks are what make `OpSelectionMerge`
//! / `OpLoopMerge` legal, and getting them wrong yields modules `spirv-val`
//! rejects (e.g. "block N branches to the selection construct, but not to the
//! selection header").
//!
//! BuildLang's MIR is a flat CFG of basic blocks with `If`/`Goto`/... terminators
//! produced by a *structured* frontend (every `if`/`while` reconverges). The old
//! SPIR-V backend guessed merge/continue targets by ad-hoc branch-following, which
//! is correct for straight-line and sequential selections but produces malformed
//! structured control flow the moment a loop (or a selection) is NESTED inside a
//! selection: it would reuse the inner loop header as the outer selection's merge.
//!
//! This module recovers the true merge/continue targets from the CFG using
//! dominator and post-dominator analysis (reusing [`super::cfg::compute_dominators`]),
//! so an arbitrary nesting of `if`/`while` validates. It is a pure function of the
//! MIR; nothing mutates.

use std::collections::{HashMap, HashSet};

use super::cfg::{block_id_index, compute_dominators, reachable_blocks, terminator_successors};
use crate::codegen::ir::{BlockId, MirBlock, MirTerminator};

/// How a conditional (`If`-terminated) header block should be lowered in
/// structured SPIR-V.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderKind {
    /// A plain selection (`if`/`if-else`). `merge` is the block both arms
    /// reconverge at (the immediate post-dominator of the header).
    Selection { merge: BlockId },
    /// A loop header (`while`). `merge` is the loop-exit block, `continue_target`
    /// the block that carries the back-edge to the header.
    Loop {
        merge: BlockId,
        continue_target: BlockId,
    },
}

/// Per-function structured-control-flow facts: for each `If`-terminated header
/// block, whether it is a selection or a loop and its merge/continue targets.
#[derive(Debug, Default)]
pub(crate) struct StructuredCfg {
    headers: HashMap<BlockId, HeaderKind>,
}

impl StructuredCfg {
    /// The structured-CFG classification for an `If`-terminated block, if it was
    /// resolved. A `None` means the header is not a recognizable structured
    /// construct (the caller falls back to a conservative merge choice).
    pub(crate) fn header(&self, block: BlockId) -> Option<HeaderKind> {
        self.headers.get(&block).copied()
    }
}

/// Analyze `blocks` and return the structured-CFG facts for every `If`-terminated
/// header. Reachable blocks only; unreachable MIR (routinely emitted by lowering)
/// is ignored.
pub(crate) fn analyze(blocks: &[MirBlock]) -> StructuredCfg {
    let n = blocks.len();
    if n == 0 {
        return StructuredCfg::default();
    }
    let id_to_index = block_id_index(blocks);
    let reachable = reachable_blocks(blocks);
    let dom = compute_dominators(blocks);
    let postdom = compute_post_dominators(blocks, &id_to_index, &reachable);

    // Successor index lists (reachable blocks only).
    let succs: Vec<Vec<usize>> = blocks
        .iter()
        .map(|b| terminator_successors(&b.terminator, &id_to_index))
        .collect();

    let mut headers = HashMap::new();

    for (h, block) in blocks.iter().enumerate() {
        if !reachable[h] {
            continue;
        }
        let Some(MirTerminator::If {
            then_block,
            else_block,
            ..
        }) = &block.terminator
        else {
            continue;
        };
        let then_i = id_to_index.get(&then_block.0).copied();
        let else_i = id_to_index.get(&else_block.0).copied();
        let (Some(then_i), Some(else_i)) = (then_i, else_i) else {
            continue;
        };

        // A loop header is a block that dominates one of its own predecessors:
        // that predecessor's edge back to `h` is a back-edge.
        let back_edge_src = back_edge_source(h, &succs, &dom, &reachable);

        if let Some(latch) = back_edge_src {
            // Loop header. In BuildLang's `while` lowering the header IS the
            // `If`: the taken arm enters the body, the other arm exits the loop.
            // The body eventually branches back to the header (the back-edge from
            // `latch`). The exit arm is whichever successor is NOT inside the loop
            // body; its target is the loop merge.
            let loop_body = loop_body_blocks(h, latch, blocks, &id_to_index, &reachable);
            let (merge_i, continue_i) =
                if loop_body.contains(&then_i) && !loop_body.contains(&else_i) {
                    (else_i, then_i)
                } else if loop_body.contains(&else_i) && !loop_body.contains(&then_i) {
                    (then_i, else_i)
                } else {
                    // Ambiguous (e.g. both or neither arm in the body). Fall back
                    // to: the arm that is the back-edge latch's dominator side is
                    // the body. Use `then` as body, `else` as exit (BuildLang's
                    // canonical `while` lowering).
                    (else_i, then_i)
                };
            headers.insert(
                block.id,
                HeaderKind::Loop {
                    merge: blocks[merge_i].id,
                    continue_target: blocks[continue_i].id,
                },
            );
        } else {
            // Selection header: the merge is the immediate post-dominator of `h`
            // (the nearest block that post-dominates the header and is not the
            // header itself). This is exactly the block both arms reconverge at.
            if let Some(merge_i) = immediate_post_dominator(h, &postdom, &reachable) {
                headers.insert(
                    block.id,
                    HeaderKind::Selection {
                        merge: blocks[merge_i].id,
                    },
                );
            }
        }
    }

    StructuredCfg { headers }
}

/// If block `h` has a reachable predecessor `p` that `h` dominates, the edge
/// `p -> h` is a back-edge and `h` is a loop header. Returns that latch `p`.
fn back_edge_source(
    h: usize,
    succs: &[Vec<usize>],
    dom: &[HashSet<usize>],
    reachable: &[bool],
) -> Option<usize> {
    for (p, ss) in succs.iter().enumerate() {
        if !reachable[p] {
            continue;
        }
        // p -> h edge and h dominates p => back-edge.
        if ss.contains(&h) && dom[p].contains(&h) {
            return Some(p);
        }
    }
    None
}

/// The set of blocks in the loop whose header is `h` and whose latch (back-edge
/// source) is `latch`: every block that can reach `latch` without passing
/// through `h` (i.e. staying inside the loop), plus `h` and `latch` themselves.
/// Computed by a backward walk from `latch` that stops at `h`.
fn loop_body_blocks(
    h: usize,
    latch: usize,
    blocks: &[MirBlock],
    id_to_index: &HashMap<u32, usize>,
    reachable: &[bool],
) -> HashSet<usize> {
    // Predecessor lists over reachable blocks.
    let n = blocks.len();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, b) in blocks.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        for s in terminator_successors(&b.terminator, id_to_index) {
            preds[s].push(i);
        }
    }
    let mut body: HashSet<usize> = HashSet::new();
    body.insert(h);
    body.insert(latch);
    let mut stack = vec![latch];
    while let Some(b) = stack.pop() {
        if b == h {
            continue;
        }
        for &p in &preds[b] {
            if body.insert(p) {
                stack.push(p);
            }
        }
    }
    body
}

/// The immediate post-dominator of `h`: the nearest block (other than `h`) that
/// post-dominates `h`. Among all post-dominators of `h` except `h`, it is the one
/// post-dominated by every other, i.e. the one with the largest post-dominator
/// set. Returns `None` if `h` has no post-dominator but itself (e.g. it always
/// reaches the exit directly, an unguarded return in both arms).
fn immediate_post_dominator(
    h: usize,
    postdom: &[HashSet<usize>],
    reachable: &[bool],
) -> Option<usize> {
    let candidates: Vec<usize> = postdom[h].iter().copied().filter(|&b| b != h).collect();
    if candidates.is_empty() {
        return None;
    }
    // The immediate post-dominator is post-dominated by all other candidates,
    // so its own post-dominator set (restricted to candidates) is the largest.
    // Equivalently: pick the candidate `c` such that every other candidate `d`
    // post-dominates `c` (d in postdom[c]).
    for &c in &candidates {
        if !reachable[c] {
            continue;
        }
        let is_ipdom = candidates
            .iter()
            .all(|&d| d == c || postdom[c].contains(&d));
        if is_ipdom {
            return Some(c);
        }
    }
    // Fallback: the candidate with the smallest post-dominator set is nearest.
    candidates
        .into_iter()
        .filter(|&c| reachable[c])
        .min_by_key(|&c| postdom[c].len())
}

/// Post-dominator sets: `postdom[i]` is the set of block indices that
/// post-dominate block `i`. `X` post-dominates `Y` iff every path from `Y` to a
/// function exit passes through `X`. Computed as dominators on the reversed CFG
/// with a virtual unique exit that all real exits (Return/Unreachable/... blocks
/// with no successors) feed into. Only reachable blocks participate; unreachable
/// predecessors would otherwise erase a join's post-dominators (fail-safe: it
/// only shrinks the sets, so post-dominance can spuriously fail, never spuriously
/// hold).
fn compute_post_dominators(
    blocks: &[MirBlock],
    id_to_index: &HashMap<u32, usize>,
    reachable: &[bool],
) -> Vec<HashSet<usize>> {
    let n = blocks.len();
    // Reverse successors = predecessors in the forward CFG. Exits (blocks with no
    // reachable successors) are the roots of the reverse graph.
    let mut rev_succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut exits: Vec<usize> = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        let ss: Vec<usize> = terminator_successors(&b.terminator, id_to_index)
            .into_iter()
            .filter(|&s| reachable[s])
            .collect();
        if ss.is_empty() {
            exits.push(i);
        }
        for s in ss {
            // reverse edge s -> i
            rev_succ[s].push(i);
        }
    }

    let all: HashSet<usize> = (0..n).collect();
    let mut postdom: Vec<HashSet<usize>> = vec![all; n];

    // Seed every exit block with just itself (each exit post-dominates itself and,
    // in the reverse graph, is a root). If there are multiple exits they share a
    // virtual sink, so no real block post-dominates across distinct exits.
    for &e in &exits {
        postdom[e] = std::iter::once(e).collect();
    }

    // reverse-predecessors of i in the reverse graph = forward successors of i.
    let rev_preds: Vec<Vec<usize>> = {
        let mut rp: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, outs) in rev_succ.iter().enumerate() {
            for &o in outs {
                rp[o].push(i);
            }
        }
        rp
    };

    let exit_set: HashSet<usize> = exits.iter().copied().collect();
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if !reachable[i] || exit_set.contains(&i) {
                continue;
            }
            // Intersect post-dominators of forward successors (rev_preds in the
            // reverse graph is exactly the forward successors).
            let mut new_set: Option<HashSet<usize>> = None;
            for &s in &rev_preds[i] {
                new_set = Some(match new_set {
                    None => postdom[s].clone(),
                    Some(acc) => acc.intersection(&postdom[s]).copied().collect(),
                });
            }
            let mut new_set = new_set.unwrap_or_default();
            new_set.insert(i);
            if new_set != postdom[i] {
                postdom[i] = new_set;
                changed = true;
            }
        }
    }
    postdom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{MirBlock, MirValue};

    fn blk(id: u32, term: MirTerminator) -> MirBlock {
        MirBlock {
            id: BlockId(id),
            label: None,
            stmts: Vec::new(),
            terminator: Some(term),
        }
    }

    fn cond() -> MirValue {
        MirValue::Const(crate::codegen::ir::MirConst::Bool(true))
    }

    /// bb0: if -> bb1(then) / bb2(else); bb1 -> bb3; bb2 -> bb3; bb3: return.
    /// A plain selection: merge must be bb3.
    #[test]
    fn plain_selection_merge_is_reconvergence() {
        let blocks = vec![
            blk(
                0,
                MirTerminator::If {
                    cond: cond(),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            blk(1, MirTerminator::Goto(BlockId(3))),
            blk(2, MirTerminator::Goto(BlockId(3))),
            blk(3, MirTerminator::Return(None)),
        ];
        let cfg = analyze(&blocks);
        assert_eq!(
            cfg.header(BlockId(0)),
            Some(HeaderKind::Selection { merge: BlockId(3) })
        );
    }

    /// The loop-in-selection shape that motivated this module:
    /// bb0: if -> bb1 / bb2   (outer selection)
    /// bb1: goto bb4          (then: enter loop init)
    /// bb2: goto bb3          (else)
    /// bb3: return            (outer merge / exit)
    /// bb4: if -> bb5 / bb6   (loop header)
    /// bb5: goto bb4          (loop body, back-edge)
    /// bb6: goto bb3          (loop exit)
    /// The OUTER selection merge must be bb3 (NOT the loop header bb4), and the
    /// loop merge must be bb6 with continue target bb5.
    #[test]
    fn loop_nested_in_selection() {
        let blocks = vec![
            blk(
                0,
                MirTerminator::If {
                    cond: cond(),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            blk(1, MirTerminator::Goto(BlockId(4))),
            blk(2, MirTerminator::Goto(BlockId(3))),
            blk(3, MirTerminator::Return(None)),
            blk(
                4,
                MirTerminator::If {
                    cond: cond(),
                    then_block: BlockId(5),
                    else_block: BlockId(6),
                },
            ),
            blk(5, MirTerminator::Goto(BlockId(4))),
            blk(6, MirTerminator::Goto(BlockId(3))),
        ];
        let cfg = analyze(&blocks);
        assert_eq!(
            cfg.header(BlockId(0)),
            Some(HeaderKind::Selection { merge: BlockId(3) }),
            "outer selection merge must be the reconvergence bb3, not the nested loop header"
        );
        assert_eq!(
            cfg.header(BlockId(4)),
            Some(HeaderKind::Loop {
                merge: BlockId(6),
                continue_target: BlockId(5),
            }),
            "loop header must name exit bb6 as merge and bb5 as continue target"
        );
    }

    /// A bare `while` loop at the top level:
    /// bb0: goto bb1; bb1: if -> bb2 / bb3; bb2: goto bb1; bb3: return.
    #[test]
    fn bare_loop() {
        let blocks = vec![
            blk(0, MirTerminator::Goto(BlockId(1))),
            blk(
                1,
                MirTerminator::If {
                    cond: cond(),
                    then_block: BlockId(2),
                    else_block: BlockId(3),
                },
            ),
            blk(2, MirTerminator::Goto(BlockId(1))),
            blk(3, MirTerminator::Return(None)),
        ];
        let cfg = analyze(&blocks);
        assert_eq!(
            cfg.header(BlockId(1)),
            Some(HeaderKind::Loop {
                merge: BlockId(3),
                continue_target: BlockId(2),
            })
        );
    }
}
