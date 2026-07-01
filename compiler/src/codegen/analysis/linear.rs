//! Increment 2b/2c: the MIR affine/borrow checker.
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
//!   `Deref` of a shared (`&`, `is_mut == false`) reference (closes class 1
//!   direct form), OR by passing a shared borrow of a linear into a callee
//!   parameter that moves the referent out and returns it (closes class 3,
//!   interprocedurally).
//!
//! # 2c additions
//!
//! - **Match-idiom / class 1 match form.** buildlang does not lower `match` to a
//!   `Switch`; it emits a `tag`-read + `Eq` + `If`-chain, and binds a variant
//!   payload with `MirRValue::VariantField { base, .. }`. A `VariantField` bind
//!   is a MOVE of its `base` (a partial move of the scrutinee, tracked
//!   conservatively as the WHOLE base moving). Matching a linear enum through a
//!   shared `&` first materializes `let s = *r` (`Deref` of the shared borrow),
//!   which the 2b move-out-of-shared-borrow rule already flags.
//! - **Class 3 (generic deref through a borrow parameter).** buildlang
//!   monomorphizes generics, but the monomorphization can erase the referent's
//!   linearity (a `deref_any::<Coin>` specializes to `deref_any_i32` with
//!   `r: &i32`), so the concrete linear never appears as a linear local inside
//!   the callee. The move-out is invisible per-function. We close it
//!   interprocedurally: a module pre-pass finds every function whose
//!   by-reference parameter's referent leaves the borrow by value — either
//!   DIRECTLY (the body derefs it and returns the dereferenced value) or
//!   TRANSITIVELY (the body passes it into an already-borrow-escaping parameter
//!   of a callee). This "borrow-escaping" set is computed as a FIXPOINT over the
//!   call graph, so a one-level wrapper `wrap(r) { deref_any(r) }` is caught
//!   even though `wrap` never derefs `r` itself (closing the transitive class-3
//!   hole). A call that passes a shared borrow of a linear (`&coin`) into such a
//!   parameter moves the referent out of the shared borrow ->
//!   `LinearMoveOutOfBorrow`. A callee that only READS through the borrow (never
//!   derefs-and-returns it and never hands it to an escaping callee position,
//!   e.g. `peek(c: &Coin) -> i64 { 0 }`) is not borrow-escaping, so the legal
//!   borrow-read case is not flagged.
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
use std::sync::Arc;

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

/// Interprocedural context threaded into the per-function check. Computed once
/// per module by `check_module`; empty for the standalone `check(func)` entry
/// point (which is then exactly the 2b per-function behavior plus the local 2c
/// idiom handling).
#[derive(Debug, Default, Clone)]
struct LinearContext {
    /// For each function NAME, the set of parameter INDICES that are
    /// "borrow-escaping": the parameter arrives by reference (`Ptr`) and the
    /// referent leaves the borrow by value out of the function — either DIRECTLY
    /// (the body derefs it and returns the dereferenced value) or TRANSITIVELY
    /// (the body passes it into an already-borrow-escaping parameter of a
    /// callee). Computed as a FIXPOINT over the call graph
    /// (`escaping_params_fixpoint`), so a one-level wrapper `wrap(r) {
    /// deref_any(r) }` is included even though `wrap` never derefs `r` itself.
    /// Passing a shared borrow of a linear into such a parameter moves the
    /// referent OUT of the borrow (class 3). A parameter that is only READ
    /// through (never deref-and-returned, never handed to an escaping callee
    /// position) is absent, so the legal borrow-read case is not flagged.
    borrow_escaping_params: HashMap<Arc<str>, HashSet<usize>>,
}

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

/// The name a `Call` terminator dispatches to, if it is a direct named call
/// (`Global` for a regular function, `Function` for a monomorphized generic).
fn call_target_name(func_val: &MirValue) -> Option<&Arc<str>> {
    match func_val {
        MirValue::Global(n) | MirValue::Function(n) => Some(n),
        _ => None,
    }
}

/// Map each by-reference (`Ptr`) parameter LOCAL of `func` to its parameter
/// INDEX. Param index == LocalId enumeration order (builder invariant: params
/// are the leading locals `0..N` in declaration order). Only pointer params are
/// candidates for borrow-escaping (a by-value param cannot leak a borrow it does
/// not hold). Shared by the direct (`borrow_escaping_params_of`) and the
/// inductive (call-graph fixpoint) escape rules.
fn ref_param_index_of(func: &MirFunction) -> HashMap<LocalId, usize> {
    func.locals
        .iter()
        .enumerate()
        .filter(|(_, l)| l.is_param && matches!(l.ty, crate::codegen::ir::MirType::Ptr(_)))
        .map(|(idx, l)| (l.id, idx))
        .collect()
}

/// The set of parameter indices of `func` that are DIRECTLY "borrow-escaping": a
/// by-reference (`Ptr`) parameter whose referent is DEREFERENCED by value and
/// RETURNED. This is the structural signature of a `fn deref_any<T>(r: &T) -> T
/// { *r }` after monomorphization (`r: &i32`, body `t = *r; return t`) — the
/// callee takes ownership of the referent out of a borrow it does not own.
///
/// The analysis is intraprocedural and purely structural (no types needed):
///   1. A parameter local `p` (`is_param`, `Ptr` type) is a candidate.
///   2. A statement `t = Deref(p)` (a by-value move-out of `*p`) taints `t`.
///   3. Tainting propagates by-value through:
///        - `t2 = Use(t)` / `Cast(t)` (a copy of a tainted local),
///        - `t2 = Aggregate { .., t, .. }` (laundering the referent into a
///          struct/tuple/enum-variant/array; field-INSENSITIVE — ANY tainted
///          operand taints the whole aggregate),
///        - `t2 = t.field` / `t.Variant._i` / `t[i]`
///          (`FieldAccess`/`VariantField`/`IndexAccess` projecting the referent
///          back out of a tainted aggregate; keyed off the BASE's taint).
///      So laundering `*r` through a struct/tuple/enum and unboxing it again
///      (`fn deref_boxed<T>(r:&T)->T { let b = Box{item:*r}; b.item }`, or the
///      tuple/enum/multi-level-wrapper variants) stays tainted end to end.
///   4. `Return(Some(t))` where `t` is tainted marks the ORIGINATING parameter
///      as borrow-escaping.
///
/// PRECISION: the taint only reaches the ORIGINATING param through `Return`. A
/// function that derefs `*r` into an aggregate but stores it LOCALLY and returns
/// a scalar / nothing (a `peek`-like fn) never returns a tainted local, so its
/// param is NOT flagged. The aggregate over-approximation (a returned aggregate
/// whose tainted field is not the returned one) still only fires on `Return`, so
/// it cannot flag a non-returning function.
///
/// This is the BASE case of the module-level fixpoint (`escaping_params_fixpoint`):
/// a param that escapes WITHOUT delegating through a callee. The INDUCTIVE case
/// (a param passed into an already-escaping callee param) is added by the
/// fixpoint, which closes the transitive one-level-wrapper hole (`wrap` -> `deref_any`).
///
/// SOUNDNESS DISPOSITION: this over-approximates "the callee moves the referent
/// out of the borrow and hands it back". It is deliberately NARROW (deref +
/// return), so a callee that only READS through the borrow (`peek(c: &Coin) ->
/// i64 { 0 }`, never derefs it) is NOT flagged — the legal borrow-read case is
/// preserved. A callee that derefs-and-returns but is called with a NON-linear,
/// NON-borrowed argument is harmless (the call-site rule only fires when the arg
/// is a shared borrow of a linear).
fn borrow_escaping_params_of(func: &MirFunction) -> HashSet<usize> {
    let mut escaping = HashSet::new();
    let Some(blocks) = &func.blocks else {
        return escaping;
    };

    // Candidate reference parameters: leading locals with `is_param` and a
    // pointer type. Param index == LocalId index (builder invariant: params are
    // locals 0..N in declaration order).
    let ref_param_index = ref_param_index_of(func);
    if ref_param_index.is_empty() {
        return escaping;
    }

    // `tainted[local] = originating param index`: `local` currently holds the
    // dereferenced referent of that reference parameter (moved out of the
    // borrow). A fixpoint over the CFG is unnecessary for the shapes buildlang
    // emits (a monomorphized `*r; return` is straight-line), but we do a simple
    // multi-pass to be robust to block ordering / a copy chain across blocks.
    let mut tainted: HashMap<LocalId, usize> = HashMap::new();
    // Taint of a single operand `MirValue`: the origin param index if it is a
    // tainted local, else `None`. Used by the aggregate-operand scan.
    let taint_of = |tainted: &HashMap<LocalId, usize>, v: &MirValue| -> Option<usize> {
        match v {
            MirValue::Local(l) => tainted.get(l).copied(),
            _ => None,
        }
    };
    let mut changed = true;
    while changed {
        changed = false;
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                    let origin = match value {
                        // t = *p : move the referent of ref-param `p` out.
                        MirRValue::Deref {
                            ptr: MirValue::Local(p),
                            ..
                        } => ref_param_index.get(p).copied(),
                        // t2 = Use(t) / Cast(t) : a by-value copy of a tainted
                        // local carries the taint forward.
                        MirRValue::Use(MirValue::Local(s))
                        | MirRValue::Cast {
                            value: MirValue::Local(s),
                            ..
                        } => tainted.get(s).copied(),
                        // t2 = Aggregate { .., t, .. } : laundering `*r` through
                        // a struct/tuple/enum-variant/array carries the taint to
                        // the aggregate. Field-INSENSITIVE and conservative: ANY
                        // tainted operand taints the WHOLE aggregate (we do not
                        // track which field holds it). If the aggregate is later
                        // returned, the param escapes — even if the returned
                        // field is not the tainted one. Over-approximates in the
                        // sound direction (see the module doc).
                        MirRValue::Aggregate { operands, .. } => {
                            operands.iter().find_map(|op| taint_of(&tainted, op))
                        }
                        // t2 = base.field / base.Variant._i / base[i] : projecting
                        // a field/variant-payload/element back OUT of a tainted
                        // aggregate re-exposes the laundered referent. Keyed off
                        // the BASE's taint (field-insensitive), so unboxing the
                        // struct in `deref_boxed` (`b.item`) stays tainted.
                        MirRValue::FieldAccess { base, .. }
                        | MirRValue::VariantField { base, .. }
                        | MirRValue::IndexAccess { base, .. } => taint_of(&tainted, base),
                        _ => None,
                    };
                    if let Some(idx) = origin {
                        if tainted.insert(*dest, idx) != Some(idx) {
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    // A `Return(Some(local))` of a tainted local escapes its origin parameter.
    for block in blocks {
        if let Some(MirTerminator::Return(Some(MirValue::Local(l)))) = &block.terminator {
            if let Some(&idx) = tainted.get(l) {
                escaping.insert(idx);
            }
        }
    }
    escaping
}

/// Compute, for EVERY function in the module, the set of borrow-escaping
/// parameter indices, as a FIXPOINT over the call graph. This is what closes the
/// transitive class-3 hole: a one-level wrapper `fn wrap<T>(r: &T) -> T {
/// deref_any(r) }` does not deref-and-return `r` itself (its body is a single
/// `Call`), so the DIRECT rule (`borrow_escaping_params_of`) misses it — yet
/// passing `&coin` into `wrap` still moves the referent out of the borrow,
/// because `wrap` hands `r` straight to `deref_any`'s already-escaping param.
///
/// - BASE case: `borrow_escaping_params_of(F)` (deref-and-return, per function).
/// - INDUCTIVE case: param `pi` of `F` is escaping if `F` contains a call
///   `G(.., arg_i, ..)` where `arg_i == Local(p)`, `p` is the reference-param
///   local at index `pi` of `F`, and position `i` of the callee `G` is ALREADY
///   in `G`'s escaping set. Callees are matched by MIR function name
///   (monomorphized/mangled: `wrap_i32` calls `deref_any_i32`); a callee's param
///   set is looked up in the growing map, defaulting to empty for unknown/extern
///   callees.
///
/// TERMINATION: the escaping sets only GROW (monotone) and are bounded by the
/// parameter count of each function; a `changed` flag stops the loop once no set
/// grows. Direct recursion (`F` calls `F`) and mutual recursion (`F` <-> `G`)
/// are handled by construction — a recursive call just reads the current set and
/// contributes monotonically, so the fixpoint still converges.
///
/// PRECISION: the inductive step only fires when the arg is EXACTLY the
/// reference parameter local passed into an escaping position. A wrapper that
/// passes `r` into a NON-escaping param (`fn w2(r:&T){ peek(r) }`, `peek` not
/// escaping) contributes nothing, so `w2` stays clean. Read-only field access
/// through the borrow never reaches an escaping callee position either.
fn escaping_params_fixpoint(functions: &[MirFunction]) -> HashMap<Arc<str>, HashSet<usize>> {
    // Seed with the direct (base-case) escaping set for every named function.
    let mut escaping: HashMap<Arc<str>, HashSet<usize>> = functions
        .iter()
        .map(|f| (f.name.clone(), borrow_escaping_params_of(f)))
        .collect();

    // Per-function: the reference-parameter local -> param index map, precomputed
    // once (the call-graph iteration reuses it every round).
    let ref_params: HashMap<Arc<str>, HashMap<LocalId, usize>> = functions
        .iter()
        .map(|f| (f.name.clone(), ref_param_index_of(f)))
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for f in functions {
            // A function with no reference parameters can never become escaping
            // via the inductive rule (nothing to leak through a callee).
            let Some(my_ref_params) = ref_params.get(&f.name) else {
                continue;
            };
            if my_ref_params.is_empty() {
                continue;
            }
            let Some(blocks) = &f.blocks else {
                continue;
            };

            // Collect the new escaping indices this round without holding a
            // mutable borrow of `escaping` while reading callee sets.
            let mut newly_escaping: Vec<usize> = Vec::new();
            for block in blocks {
                let Some(MirTerminator::Call {
                    func: callee, args, ..
                }) = &block.terminator
                else {
                    continue;
                };
                let Some(callee_name) = call_target_name(callee) else {
                    continue;
                };
                let Some(callee_escaping) = escaping.get(callee_name) else {
                    continue;
                };
                for (i, a) in args.iter().enumerate() {
                    // The arg is one of MY reference parameters, passed into an
                    // ALREADY-escaping position of the callee -> MY param escapes.
                    if let MirValue::Local(p) = a {
                        if let Some(&my_idx) = my_ref_params.get(p) {
                            if callee_escaping.contains(&i) {
                                newly_escaping.push(my_idx);
                            }
                        }
                    }
                }
            }

            if !newly_escaping.is_empty() {
                let my_set = escaping.entry(f.name.clone()).or_default();
                for idx in newly_escaping {
                    if my_set.insert(idx) {
                        changed = true;
                    }
                }
            }
        }
    }

    // Drop empty sets: the call-site rule only consults present entries, and an
    // empty set is indistinguishable from absent. Keeps the context minimal.
    escaping.retain(|_, set| !set.is_empty());
    escaping
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
/// as an aggregate operand / cast / repeat operand (by-value composition), or a
/// `VariantField` bind that destructures a payload OUT of `L` (a partial move,
/// tracked conservatively as the whole scrutinee moving — 2c match idiom). A
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
        // 2c match idiom: `dest = VariantField { base }` binds a variant payload
        // out of `base`. buildlang lowers a `match` arm's payload bind to this,
        // so extracting the payload CONSUMES the scrutinee. Moving one variant
        // field out of an owned scrutinee is a partial move; we conservatively
        // treat the WHOLE `base` as moved (sound: a later use of a DIFFERENT
        // field is over-rejected, but no unsound program passes). This makes the
        // owned-match-payload case a move of the scrutinee; the match-through-`&`
        // case additionally trips the `Deref`-of-shared-borrow rule at the
        // `let s = *r` statement lowering emits before the bind.
        MirRValue::VariantField { base, .. } => push_if_linear(base),
        // Borrows and reads do NOT move.
        MirRValue::Ref { .. }
        | MirRValue::AddressOf { .. }
        | MirRValue::FieldAccess { .. }
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
        MirRValue::FieldAccess { base, .. } => push_val(func, base, out),
        // `VariantField` is a MOVE of its base (`moved_by_rvalue`), not a
        // non-consuming read, so it is intentionally absent here.
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
        // A plain `Use`/aggregate/cast/repeat/variant-field is a MOVE (handled
        // by `moved_by_rvalue`), not a non-consuming read.
        MirRValue::Use(_)
        | MirRValue::Aggregate { .. }
        | MirRValue::Repeat { .. }
        | MirRValue::Cast { .. }
        | MirRValue::VariantField { .. }
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

/// Run the whole-module affine/borrow check. Computes the interprocedural
/// context (borrow-escaping parameters, for class 3) once, then runs the
/// per-function check with it. This is the entry point the compile pipeline
/// uses; `check(func)` remains for direct per-function checks (unit tests) and
/// is exactly `check_module` with an empty context.
pub(crate) fn check_module(functions: &[MirFunction]) -> Vec<TypeErrorWithSpan> {
    // Borrow-escaping parameters, computed as a FIXPOINT over the call graph so
    // a transitive wrapper (`wrap(&coin)` delegating to `deref_any`) is caught,
    // not just the direct deref-and-return shape.
    let ctx = LinearContext {
        borrow_escaping_params: escaping_params_fixpoint(functions),
    };
    functions
        .iter()
        .flat_map(|f| check_with_context(f, &ctx))
        .collect()
}

/// Run the affine/borrow check over one MIR function with no interprocedural
/// context. Pure per-function entry point (unit tests, direct callers). Class 3
/// (borrow-escaping-parameter) detection needs `check_module`; without context
/// this is the 2b behavior plus the local 2c match idiom.
pub(crate) fn check(func: &MirFunction) -> Vec<TypeErrorWithSpan> {
    check_with_context(func, &LinearContext::default())
}

/// Run the affine/borrow check over one MIR function, given the module context.
/// Pure: no mutation, no side effects. Returns every violation with its
/// statement-level span.
///
/// The transfer function walks each block forward from a per-block entry state
/// (the join of predecessor exit states). For every statement/terminator, in
/// order: process READS (borrow-after-move), MOVE-OUT-OF-BORROW, then MOVES
/// (use-after-move + `Init -> Moved`), then the DEFINITION reset (`-> Init`).
/// Iterating to a fixpoint over the CFG lets the maybe-moved join propagate
/// across back-edges and merges. Diagnostics are collected on a final,
/// deterministic forward pass over the converged entry states so each site is
/// reported at most once in stable order.
fn check_with_context(func: &MirFunction, ctx: &LinearContext) -> Vec<TypeErrorWithSpan> {
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
            transfer_block(func, ctx, &blocks[i], &shared_refs, &mut state, None);
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
            ctx,
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
    ctx: &LinearContext,
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
    apply_terminator(
        func,
        ctx,
        &block.terminator,
        shared_refs,
        state,
        span,
        errors.as_deref_mut(),
    );
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
///
/// Class 3 (interprocedural, needs `ctx`): if an arg is a shared borrow of a
/// linear (`p in shared_refs`) and the callee's corresponding parameter is
/// BORROW-ESCAPING (it derefs-and-returns the referent), the call moves the
/// linear OUT of the shared borrow -> `LinearMoveOutOfBorrow`.
fn apply_terminator(
    func: &MirFunction,
    ctx: &LinearContext,
    term: &Option<MirTerminator>,
    shared_refs: &HashSet<LocalId>,
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
            // Class 3: a shared borrow of a linear passed into a borrow-escaping
            // parameter is moved out of the borrow by the callee.
            if let Some(name) = call_target_name(callee) {
                if let Some(escaping) = ctx.borrow_escaping_params.get(name) {
                    for (i, a) in args.iter().enumerate() {
                        if let MirValue::Local(p) = a {
                            if escaping.contains(&i) && shared_refs.contains(p) {
                                if let Some(errs) = errors.as_deref_mut() {
                                    errs.push(TypeErrorWithSpan::new(
                                        TypeError::LinearMoveOutOfBorrow {
                                            name: local_name(func, *p),
                                        },
                                        span,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
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
        BlockId, LocalId, MirBlock, MirConst, MirFnSig, MirFunction, MirLocal, MirRValue, MirStmt,
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

    /// A `#[linear]` local named `_N` (anonymous), struct type `Qubit`. Used for
    /// the compiler-generated scrutinee copies a `match` produces.
    fn linear_anon(id: u32) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: None,
            ty: MirType::Struct(Arc::from("Qubit")),
            is_mut: false,
            is_param: false,
            annotations: vec![Arc::from("linear")],
        }
    }

    /// A by-REFERENCE parameter local (`is_param`, `Ptr(Struct("Qubit"))`, empty
    /// annotations) — the `r: &T` a monomorphized generic like `deref_any`
    /// receives. Param index == its LocalId (builder invariant).
    fn ref_param_local(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::Ptr(Box::new(MirType::Struct(Arc::from("Qubit")))),
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        }
    }

    /// A `VariantField` bind of variant `Full` field 0 out of `base`.
    fn variant_field(base: LocalId) -> MirRValue {
        MirRValue::VariantField {
            base: MirValue::Local(base),
            variant_name: Arc::from("Full"),
            field_index: 0,
            field_ty: MirType::Struct(Arc::from("Coin")),
        }
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

    // ===================================================================
    // 2c: match idiom, class 1/3, and precision.
    // ===================================================================

    // 11. MATCH IDIOM (owned): a `VariantField` bind out of an OWNED linear
    //     scrutinee copy MOVES the scrutinee; a later use of the scrutinee is a
    //     use-after-move. This is the `match w { Full(c) => ... }` shape: the
    //     scrutinee `_4 = Use(w)` is copied, then `c = VariantField(_4)` extracts
    //     the payload (consuming `_4`), and a second `VariantField(_4)` is a
    //     double-consume.
    //     bb0: _1 = Use(_0) ; _2 = VariantField(_1) ; _3 = VariantField(_1) ; ret
    #[test]
    fn owned_variant_field_bind_moves_scrutinee() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "w")); // owned linear scrutinee
        func.locals.push(linear_anon(1)); // scrutinee copy `_1 = Use(w)`
        func.locals.push(i64_local(2, "c1"));
        func.locals.push(i64_local(3, "c2"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        // first payload bind out of the scrutinee copy (legal: moves _1 once)
        b0.stmts
            .push(MirStmt::assign(LocalId(2), variant_field(LocalId(1))));
        // second payload bind out of the SAME scrutinee copy -> use-after-move
        b0.stmts
            .push(MirStmt::assign(LocalId(3), variant_field(LocalId(1))));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearUseAfterMove { .. }
            )),
            1,
            "the second VariantField bind double-consumes the scrutinee: {errors:?}"
        );
    }

    // 12. MATCH IDIOM (owned) is CLEAN when the payload is bound exactly once.
    //     A single `VariantField` bind is a legal (first) move of the scrutinee.
    //     bb0: _1 = Use(_0) ; _2 = VariantField(_1) ; return
    #[test]
    fn owned_variant_field_bind_once_is_clean() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "w"));
        func.locals.push(linear_anon(1));
        func.locals.push(i64_local(2, "c"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b0.stmts
            .push(MirStmt::assign(LocalId(2), variant_field(LocalId(1))));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert!(
            errors.is_empty(),
            "a single payload bind is a legal move of the scrutinee: {errors:?}"
        );
    }

    // 13. MATCH-THROUGH-`&` (class 1 match form): buildlang lowers `match &w`
    //     (linear enum) to `r = &w ; s = *r ; ... VariantField(s)`. The `s = *r`
    //     Deref of a shared borrow of a linear is the move-out-of-shared-borrow,
    //     flagged at that statement — even before the VariantField bind. This is
    //     the exact MIR the compiler emits (see the class1 repro).
    //     bb0: r = &coin ; s = Deref(r) ; c = VariantField(s) ; return
    #[test]
    fn match_through_shared_borrow_reports_move_out_of_borrow() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "w")); // linear enum scrutinee
        func.locals.push(untagged_ptr_local(1, "r")); // r = &w
        func.locals.push(linear_anon(2)); // s = *r (derefed copy)
        func.locals.push(i64_local(3, "c"));
        let mut b0 = MirBlock::new(BlockId(0));
        // r = &w (shared borrow of a linear -> provenance set)
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: false,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        // s = *r (move out of the shared borrow)
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(1)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        // c = VariantField(s) (payload bind out of the derefed copy)
        b0.stmts
            .push(MirStmt::assign(LocalId(3), variant_field(LocalId(2))));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "matching a linear through a shared borrow moves it out of the borrow: {errors:?}"
        );
    }

    // 14. CLASS 3 (interprocedural): passing a shared borrow of a linear into a
    //     BORROW-ESCAPING parameter (a callee that derefs-and-returns its `&`
    //     param) moves the referent out of the borrow.
    //     callee `deref_any`: r:&Qubit (param0) ; t = *r ; return t
    //     caller `main`: r = &coin ; deref_any(r) -> LinearMoveOutOfBorrow
    #[test]
    fn class3_pass_shared_borrow_into_borrow_escaping_param_reports() {
        // Callee: fn deref_any(r: &Qubit) -> Qubit { *r }
        let mut callee = MirFunction::new("deref_any", MirFnSig::new(vec![], MirType::Void));
        callee.locals.push(ref_param_local(0, "r")); // param 0: &Qubit
        callee.locals.push(linear_anon(1)); // t = *r
        let mut cb0 = MirBlock::new(BlockId(0));
        cb0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(0)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        cb0.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        callee.blocks = Some(vec![cb0]);

        // Caller: r = &coin ; deref_any(r)
        let mut caller = MirFunction::new("main", MirFnSig::new(vec![], MirType::Void));
        caller.locals.push(linear_local(0, "coin"));
        caller.locals.push(untagged_ptr_local(1, "r")); // r = &coin
        caller.locals.push(linear_anon(2)); // call dest
        let mut mb0 = MirBlock::new(BlockId(0));
        mb0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: false,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        mb0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("deref_any")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: Some(LocalId(2)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut mb1 = MirBlock::new(BlockId(1));
        mb1.terminator = Some(MirTerminator::Return(None));
        caller.blocks = Some(vec![mb0, mb1]);

        let errors = check_module(&[callee, caller]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "passing `&coin` into a deref-and-return param moves it out of the borrow: {errors:?}"
        );
    }

    // 15. CLASS 3 negative: a callee that only READS through its `&` parameter
    //     (never derefs-and-returns it) is NOT borrow-escaping, so passing
    //     `&coin` to it is the LEGAL borrow-read case and is not flagged.
    //     callee `peek`: r:&Qubit (param0) ; return 0
    //     caller: r = &coin ; peek(r) -> clean
    #[test]
    fn class3_read_only_borrow_param_is_clean() {
        // Callee: fn peek(r: &Qubit) -> i64 { 0 }  (never derefs r)
        let mut callee = MirFunction::new("peek", MirFnSig::new(vec![], MirType::Void));
        callee.locals.push(ref_param_local(0, "r"));
        let mut cb0 = MirBlock::new(BlockId(0));
        cb0.terminator = Some(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
            0,
            MirType::i64(),
        )))));
        callee.blocks = Some(vec![cb0]);

        // Caller: r = &coin ; peek(r)
        let mut caller = MirFunction::new("main", MirFnSig::new(vec![], MirType::Void));
        caller.locals.push(linear_local(0, "coin"));
        caller.locals.push(untagged_ptr_local(1, "r"));
        caller.locals.push(i64_local(2, "res"));
        let mut mb0 = MirBlock::new(BlockId(0));
        mb0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: false,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        mb0.terminator = Some(MirTerminator::Call {
            func: MirValue::Global(Arc::from("peek")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: Some(LocalId(2)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut mb1 = MirBlock::new(BlockId(1));
        mb1.terminator = Some(MirTerminator::Return(None));
        caller.blocks = Some(vec![mb0, mb1]);

        let errors = check_module(&[callee, caller]);
        assert!(
            errors.is_empty(),
            "a read-only borrow parameter is the legal borrow case; not flagged: {errors:?}"
        );
    }

    // ===================================================================
    // 2c PRECISION (the payoff): safe compositional shapes the name-keyed AST
    // tracker over-rejects must CHECK CLEAN on the MIR checker (zero errors).
    // The AST tracker is still active at the CLI (2d removes it), so these are
    // proven here directly against `check`.
    // ===================================================================

    // 16. SAFE: a linear in a TUPLE, used once.
    //     `let t = (coin, 7) ; spend(t.0)` lowers to a tuple Aggregate then a
    //     field read + move of the linear operand. Consumed exactly once.
    //     bb0: _1 = coin ; _2 = Aggregate(_1, 7) ; call spend(_1) ; return
    #[test]
    fn precision_linear_in_tuple_used_once_is_clean() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "coin"));
        func.locals.push(i64_local(1, "seven"));
        func.locals.push(i64_local(2, "tuple")); // the tuple aggregate
        let mut b0 = MirBlock::new(BlockId(0));
        // t = (coin, 7): the linear operand is moved into the tuple once.
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Aggregate {
                kind: crate::codegen::ir::AggregateKind::Tuple,
                operands: vec![MirValue::Local(LocalId(0)), MirValue::Local(LocalId(1))],
            },
        ));
        b0.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0]);

        let errors = check(&func);
        assert!(
            errors.is_empty(),
            "a linear moved into a tuple exactly once is safe: {errors:?}"
        );
    }

    // 17. SAFE: `Option<Linear>` / `Ok(q)` constructed and consumed once.
    //     `let o = Some(coin) ; consume(o)` lowers to an enum-variant Aggregate
    //     (moving the linear in once) then a by-value consume of the option.
    //     bb0: _1 = coin ; _2 = Aggregate::Variant(Some, _1) ; call consume(_2)
    #[test]
    fn precision_option_of_linear_once_is_clean() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "coin"));
        func.locals.push(linear_anon(1)); // the Option<Coin> value (still linear)
        func.locals.push(i64_local(2, "res"));
        let mut b0 = MirBlock::new(BlockId(0));
        // o = Some(coin): the linear is moved into the variant payload once.
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Aggregate {
                kind: crate::codegen::ir::AggregateKind::Variant(
                    Arc::from("Option"),
                    0,
                    Arc::from("Some"),
                ),
                operands: vec![MirValue::Local(LocalId(0))],
            },
        ));
        // consume(o): by-value consume of the option once.
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Global(Arc::from("consume")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: Some(LocalId(2)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1]);

        let errors = check(&func);
        assert!(
            errors.is_empty(),
            "constructing Some(coin) and consuming it once is safe: {errors:?}"
        );
    }

    // 18. SAFE: `fn id<T>(x: T) -> T { x }` applied to a linear, then consumed.
    //     After monomorphization `id_Coin(coin)` returns a fresh owned linear
    //     bound to `c2`; the call MOVES `coin` once (arg by value) and REBINDS
    //     `c2` (call dest); `spend(c2)` consumes `c2` once. `id` is NOT
    //     borrow-escaping (it takes `x: T` by value, not `&T`), so no class-3
    //     flag.
    //     bb0: _0 = coin ; call id(_0) -> dest _1 ; bb1: call spend(_1) ; return
    #[test]
    fn precision_generic_identity_over_linear_is_clean() {
        // Callee: fn id(x: Qubit) -> Qubit { x }  (by-VALUE param, not a ref)
        let mut callee = MirFunction::new("id", MirFnSig::new(vec![], MirType::Void));
        callee.locals.push(linear_local(0, "x")); // by-value linear param
        let mut cb0 = MirBlock::new(BlockId(0));
        cb0.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(0)))));
        callee.blocks = Some(vec![cb0]);

        // Caller: c2 = id(coin) ; spend(c2)
        let mut caller = MirFunction::new("main", MirFnSig::new(vec![], MirType::Void));
        caller.locals.push(linear_local(0, "coin"));
        caller.locals.push(linear_anon(1)); // c2 (call dest, fresh)
        caller.locals.push(i64_local(2, "r"));
        let mut mb0 = MirBlock::new(BlockId(0));
        mb0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("id")),
            args: vec![MirValue::Local(LocalId(0))],
            dest: Some(LocalId(1)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut mb1 = MirBlock::new(BlockId(1));
        mb1.terminator = Some(MirTerminator::Call {
            func: MirValue::Global(Arc::from("spend")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: Some(LocalId(2)),
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut mb2 = MirBlock::new(BlockId(2));
        mb2.terminator = Some(MirTerminator::Return(None));
        caller.blocks = Some(vec![mb0, mb1, mb2]);

        let errors = check_module(&[callee, caller]);
        assert!(
            errors.is_empty(),
            "generic identity over a linear, consumed once, is safe: {errors:?}"
        );
    }

    // 19. SAFE: a closure that moves a linear once. The linear is captured by
    //     value into a closure Aggregate (moved once), and the closure is called
    //     once. No double-consume.
    //     bb0: _0 = coin ; _1 = Aggregate::Closure(_0) ; call _1 ; return
    #[test]
    fn precision_closure_moving_linear_once_is_clean() {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(linear_local(0, "coin"));
        func.locals.push(i64_local(1, "clos")); // the closure environment value
        func.locals.push(i64_local(2, "res"));
        let mut b0 = MirBlock::new(BlockId(0));
        // clos = move |...| { ... coin ... }: capture the linear by value once.
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Aggregate {
                kind: crate::codegen::ir::AggregateKind::Closure(Arc::from("closure_0")),
                operands: vec![MirValue::Local(LocalId(0))],
            },
        ));
        // call the closure once.
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Local(LocalId(1)),
            args: vec![],
            dest: Some(LocalId(2)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1]);

        let errors = check(&func);
        assert!(
            errors.is_empty(),
            "a closure capturing a linear by value once is safe: {errors:?}"
        );
    }

    // ===================================================================
    // 2c FIXPOINT (transitive class-3 hole): the borrow-escaping-parameter
    // analysis must close over the CALL GRAPH. A one-level wrapper that hands
    // its `&` param to an escaping callee is ALSO borrow-escaping, so passing
    // `&linear` into the wrapper is a move-out-of-borrow. Precision is
    // preserved: a wrapper delegating to a NON-escaping callee is not flagged.
    // ===================================================================

    /// Build a `deref_any`-shaped callee: `fn NAME(r: &Qubit) -> Qubit { *r }`
    /// (param 0 is directly borrow-escaping — derefs and returns the referent).
    fn deref_and_return_callee(name: &str) -> MirFunction {
        let mut f = MirFunction::new(name, MirFnSig::new(vec![], MirType::Void));
        f.locals.push(ref_param_local(0, "r")); // param 0: &Qubit
        f.locals.push(linear_anon(1)); // t = *r
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(0)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        f.blocks = Some(vec![b0]);
        f
    }

    /// Build a `peek`-shaped read-only callee: `fn NAME(r: &Qubit) -> i64 { 0 }`
    /// (never derefs r; NOT borrow-escaping).
    fn read_only_callee(name: &str) -> MirFunction {
        let mut f = MirFunction::new(name, MirFnSig::new(vec![], MirType::Void));
        f.locals.push(ref_param_local(0, "r"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
            0,
            MirType::i64(),
        )))));
        f.blocks = Some(vec![b0]);
        f
    }

    /// Build a wrapper `fn NAME(r: &Qubit) -> Qubit { CALLEE(r) }`: its param 0
    /// (a `&`) is forwarded straight into arg-position 0 of `callee`. It NEVER
    /// derefs `r` itself, so the direct escape rule misses it; the fixpoint must
    /// mark it escaping iff `callee`'s param 0 is escaping.
    ///   bb0: call CALLEE(r) -> dest _1 ; bb1: return _1
    fn forwarding_wrapper(name: &str, callee: &str) -> MirFunction {
        let mut f = MirFunction::new(name, MirFnSig::new(vec![], MirType::Void));
        f.locals.push(ref_param_local(0, "r")); // param 0: &Qubit
        f.locals.push(linear_anon(1)); // call dest (the returned value)
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from(callee)),
            args: vec![MirValue::Local(LocalId(0))],
            dest: Some(LocalId(1)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        f.blocks = Some(vec![b0, b1]);
        f
    }

    /// Build a `main` that does `r = &coin ; NAME(r)` (a single call passing a
    /// shared borrow of a linear into `callee`'s param 0). Distinct dest local.
    fn caller_passing_shared_borrow(callee: &str) -> MirFunction {
        let mut caller = MirFunction::new("main", MirFnSig::new(vec![], MirType::Void));
        caller.locals.push(linear_local(0, "coin"));
        caller.locals.push(untagged_ptr_local(1, "r")); // r = &coin
        caller.locals.push(linear_anon(2)); // call dest
        let mut mb0 = MirBlock::new(BlockId(0));
        mb0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Ref {
                is_mut: false,
                place: MirPlace::local(LocalId(0)),
            },
        ));
        mb0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from(callee)),
            args: vec![MirValue::Local(LocalId(1))],
            dest: Some(LocalId(2)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut mb1 = MirBlock::new(BlockId(1));
        mb1.terminator = Some(MirTerminator::Return(None));
        caller.blocks = Some(vec![mb0, mb1]);
        caller
    }

    // 20. FIXPOINT (the fix): a wrapper `wrap` that FORWARDS its `&` param to
    //     `deref_any` (an escaping callee) must be computed borrow-escaping by
    //     the module fixpoint, even though `wrap` never derefs `r` itself.
    #[test]
    fn fixpoint_forwarding_wrapper_param_is_borrow_escaping() {
        let deref_any = deref_and_return_callee("deref_any");
        let wrap = forwarding_wrapper("wrap", "deref_any");
        let escaping = escaping_params_fixpoint(&[deref_any, wrap]);

        assert_eq!(
            escaping.get("deref_any"),
            Some(&HashSet::from([0])),
            "the direct deref-and-return param is the base case: {escaping:?}"
        );
        assert_eq!(
            escaping.get("wrap"),
            Some(&HashSet::from([0])),
            "the wrapper forwarding into an escaping callee must be escaping too: {escaping:?}"
        );
    }

    // 21. FIXPOINT end-to-end: passing `&coin` into the wrapper `wrap` (which
    //     delegates to `deref_any`) moves the referent out of the shared borrow.
    //     This is the transitive class-3 double-spend hole, now closed.
    #[test]
    fn fixpoint_pass_shared_borrow_into_wrapper_reports() {
        let deref_any = deref_and_return_callee("deref_any");
        let wrap = forwarding_wrapper("wrap", "deref_any");
        let main = caller_passing_shared_borrow("wrap");

        let errors = check_module(&[deref_any, wrap, main]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "passing `&coin` into a transitive wrapper moves it out of the borrow: {errors:?}"
        );
    }

    // 22. PRECISION: a wrapper `w2` that forwards its `&` param to a NON-escaping
    //     callee (`peek`, read-only) must NOT become borrow-escaping, and passing
    //     `&coin` into `w2` must stay CLEAN.
    #[test]
    fn fixpoint_wrapper_over_non_escaping_callee_stays_clean() {
        let peek = read_only_callee("peek");
        let w2 = forwarding_wrapper("w2", "peek");
        let main = caller_passing_shared_borrow("w2");

        let escaping = escaping_params_fixpoint(&[peek.clone(), w2.clone()]);
        assert!(
            escaping.get("peek").is_none(),
            "a read-only borrow param is not escaping: {escaping:?}"
        );
        assert!(
            escaping.get("w2").is_none(),
            "a wrapper over a non-escaping callee is not escaping: {escaping:?}"
        );

        let errors = check_module(&[peek, w2, main]);
        assert!(
            errors.is_empty(),
            "passing `&coin` into a non-escaping wrapper is the legal borrow case: {errors:?}"
        );
    }

    // 23. FIXPOINT depth: a two-level wrapper chain `wrap2 -> wrap -> deref_any`
    //     must propagate escaping all the way up (proves the fixpoint iterates,
    //     not just one hop), so passing `&coin` into `wrap2` is rejected.
    #[test]
    fn fixpoint_two_level_wrapper_reports() {
        let deref_any = deref_and_return_callee("deref_any");
        let wrap = forwarding_wrapper("wrap", "deref_any");
        let wrap2 = forwarding_wrapper("wrap2", "wrap");

        let escaping = escaping_params_fixpoint(&[deref_any.clone(), wrap.clone(), wrap2.clone()]);
        assert_eq!(
            escaping.get("wrap2"),
            Some(&HashSet::from([0])),
            "escaping must propagate two hops up the wrapper chain: {escaping:?}"
        );

        let main = caller_passing_shared_borrow("wrap2");
        let errors = check_module(&[deref_any, wrap, wrap2, main]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "passing `&coin` into a two-level wrapper moves it out of the borrow: {errors:?}"
        );
    }

    // 24. FIXPOINT termination: a self-recursive wrapper (`rec` calls `rec` with
    //     its own `&` param, and also forwards to `deref_any`) must converge (no
    //     infinite loop) and be marked escaping. Guards against a call-graph
    //     cycle diverging the fixpoint.
    //     fn rec(r: &Qubit) -> Qubit { if _ { rec(r) } else { deref_any(r) } }
    #[test]
    fn fixpoint_recursive_wrapper_terminates_and_reports() {
        let deref_any = deref_and_return_callee("deref_any");

        let mut rec = MirFunction::new("rec", MirFnSig::new(vec![], MirType::Void));
        rec.locals.push(ref_param_local(0, "r")); // param 0: &Qubit
        rec.locals.push(i64_local(9, "cond"));
        rec.locals.push(linear_anon(1)); // dest of the recursive call
        rec.locals.push(linear_anon(2)); // dest of the deref_any call
        let mut rb0 = MirBlock::new(BlockId(0));
        rb0.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        });
        // then: t = rec(r) ; return t   (direct recursion on the &-param)
        let mut rb1 = MirBlock::new(BlockId(1));
        rb1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("rec")),
            args: vec![MirValue::Local(LocalId(0))],
            dest: Some(LocalId(1)),
            target: Some(BlockId(3)),
            unwind: None,
        });
        // else: t = deref_any(r) ; return t
        let mut rb2 = MirBlock::new(BlockId(2));
        rb2.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("deref_any")),
            args: vec![MirValue::Local(LocalId(0))],
            dest: Some(LocalId(2)),
            target: Some(BlockId(3)),
            unwind: None,
        });
        let mut rb3 = MirBlock::new(BlockId(3));
        rb3.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        rec.blocks = Some(vec![rb0, rb1, rb2, rb3]);

        // Terminates (monotone growth + `changed` flag), and `rec` param 0 is
        // escaping via the `deref_any(r)` arm.
        let escaping = escaping_params_fixpoint(&[deref_any.clone(), rec.clone()]);
        assert_eq!(
            escaping.get("rec"),
            Some(&HashSet::from([0])),
            "a recursive wrapper forwarding to an escaping callee is escaping: {escaping:?}"
        );

        let main = caller_passing_shared_borrow("rec");
        let errors = check_module(&[deref_any, rec, main]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "passing `&coin` into the recursive wrapper is rejected: {errors:?}"
        );
    }

    // ===================================================================
    // FIX A (aggregate-launder class 3): the borrow-escape taint must follow
    // `*r` through an Aggregate (struct/tuple/enum-variant) and back out through
    // a FieldAccess / VariantField / IndexAccess projection. Otherwise a generic
    // `deref_boxed<T>(r:&T)->T { let b = Box{item:*r}; b.item }` launders the
    // referent past the deref-and-return signature and clones a linear. Precision
    // preserved: a fn that boxes `*r` locally but returns a scalar stays clean.
    // ===================================================================

    /// A struct-aggregate operand of `Box_i32` over one operand local.
    fn box_aggregate(operand: LocalId) -> MirRValue {
        MirRValue::Aggregate {
            kind: crate::codegen::ir::AggregateKind::Struct(Arc::from("Box_i32")),
            operands: vec![MirValue::Local(operand)],
        }
    }

    /// `fn deref_boxed(r:&Qubit) -> Qubit { let b = Box{item:*r}; b.item }`,
    /// exactly the monomorphized MIR the compiler emits (verified via `mir emit`):
    ///   _2 = *r ; _3 = Aggregate{Box}(_2) ; _4 = Use(_3) ; _5 = _4.item ; ret _5
    /// The deref taints `_2`, the aggregate carries taint to `_3`, `Use` to `_4`,
    /// the field-access re-exposes it in `_5`, and the return escapes param 0.
    fn deref_boxed_struct_callee(name: &str) -> MirFunction {
        let mut f = MirFunction::new(name, MirFnSig::new(vec![], MirType::Void));
        f.locals.push(ref_param_local(0, "r")); // param 0: &Qubit
        f.locals.push(linear_anon(2)); // _2 = *r
        f.locals.push(i64_local(3, "box")); // _3 = Box { item: _2 }
        f.locals.push(i64_local(4, "b")); // _4 = Use(_3)
        f.locals.push(linear_anon(5)); // _5 = _4.item
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(0)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.stmts
            .push(MirStmt::assign(LocalId(3), box_aggregate(LocalId(2))));
        b0.stmts.push(MirStmt::assign(
            LocalId(4),
            MirRValue::Use(MirValue::Local(LocalId(3))),
        ));
        b0.stmts.push(MirStmt::assign(
            LocalId(5),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(4)),
                field_name: Arc::from("item"),
                field_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(5)))));
        f.blocks = Some(vec![b0]);
        f
    }

    // A1. FIX A: the struct-launder callee `deref_boxed` must be detected
    //     borrow-escaping (taint follows `*r` into the aggregate and back out
    //     through the field access).
    #[test]
    fn fixa_deref_boxed_struct_is_borrow_escaping() {
        let f = deref_boxed_struct_callee("deref_boxed");
        let escaping = escaping_params_fixpoint(&[f]);
        assert_eq!(
            escaping.get("deref_boxed"),
            Some(&HashSet::from([0])),
            "taint must follow *r through the struct aggregate and field-access: {escaping:?}"
        );
    }

    // A2. FIX A end-to-end: `deref_boxed(&coin)` (struct launder) rejects.
    #[test]
    fn fixa_pass_shared_borrow_into_struct_launder_reports() {
        let f = deref_boxed_struct_callee("deref_boxed");
        let main = caller_passing_shared_borrow("deref_boxed");
        let errors = check_module(&[f, main]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "passing `&coin` into a struct-laundering deref-return clones the linear: {errors:?}"
        );
    }

    // A3. FIX A (tuple): `fn deref_tup(r:&T)->T { let t=(*r,); t.0 }` launders
    //     through a tuple Aggregate + IndexAccess-shaped/field projection. Model
    //     the payload extract as a FieldAccess of tuple field 0 (same base-taint
    //     path); the essential edge is Aggregate + projection.
    #[test]
    fn fixa_deref_tuple_is_borrow_escaping_and_reports() {
        let mut f = MirFunction::new("deref_tup", MirFnSig::new(vec![], MirType::Void));
        f.locals.push(ref_param_local(0, "r"));
        f.locals.push(linear_anon(1)); // _1 = *r
        f.locals.push(i64_local(2, "tup")); // _2 = (*r,) tuple aggregate
        f.locals.push(linear_anon(3)); // _3 = tup.0
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(0)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Aggregate {
                kind: crate::codegen::ir::AggregateKind::Tuple,
                operands: vec![MirValue::Local(LocalId(1))],
            },
        ));
        b0.stmts.push(MirStmt::assign(
            LocalId(3),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(2)),
                field_name: Arc::from("0"),
                field_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(3)))));
        f.blocks = Some(vec![b0]);

        let escaping = escaping_params_fixpoint(std::slice::from_ref(&f));
        assert_eq!(
            escaping.get("deref_tup"),
            Some(&HashSet::from([0])),
            "taint must follow *r through a tuple aggregate and projection: {escaping:?}"
        );
        let main = caller_passing_shared_borrow("deref_tup");
        let errors = check_module(&[f, main]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "the tuple launder clones the linear: {errors:?}"
        );
    }

    // A4. FIX A (enum): `fn deref_enum(r:&T)->T { let e=Some(*r); match e {..} }`
    //     launders through an enum-variant Aggregate and extracts it back with a
    //     VariantField bind. The variant-field projection must carry the taint.
    #[test]
    fn fixa_deref_enum_is_borrow_escaping_and_reports() {
        let mut f = MirFunction::new("deref_enum", MirFnSig::new(vec![], MirType::Void));
        f.locals.push(ref_param_local(0, "r"));
        f.locals.push(linear_anon(1)); // _1 = *r
        f.locals.push(linear_anon(2)); // _2 = Some(*r) enum-variant aggregate
        f.locals.push(linear_anon(3)); // _3 = VariantField(_2)
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(0)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Aggregate {
                kind: crate::codegen::ir::AggregateKind::Variant(
                    Arc::from("Option"),
                    0,
                    Arc::from("Some"),
                ),
                operands: vec![MirValue::Local(LocalId(1))],
            },
        ));
        b0.stmts
            .push(MirStmt::assign(LocalId(3), variant_field(LocalId(2))));
        b0.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(3)))));
        f.blocks = Some(vec![b0]);

        let escaping = escaping_params_fixpoint(std::slice::from_ref(&f));
        assert_eq!(
            escaping.get("deref_enum"),
            Some(&HashSet::from([0])),
            "taint must follow *r through an enum-variant aggregate and VariantField: {escaping:?}"
        );
        let main = caller_passing_shared_borrow("deref_enum");
        let errors = check_module(&[f, main]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "the enum launder clones the linear: {errors:?}"
        );
    }

    // A5. FIX A (3-level wrapper chain): `w2 -> w1 -> launder`, where `launder`
    //     is the struct-launder callee. The fixpoint over the call graph must
    //     propagate escaping from the aggregate-launder base all the way up two
    //     forwarding hops, so passing `&coin` into `w2` is rejected. This proves
    //     FIX A (aggregate base case) composes with the existing call-graph
    //     fixpoint (transitive case).
    #[test]
    fn fixa_three_level_wrapper_over_launder_reports() {
        let launder = deref_boxed_struct_callee("launder");
        let w1 = forwarding_wrapper("w1", "launder");
        let w2 = forwarding_wrapper("w2", "w1");

        let escaping = escaping_params_fixpoint(&[launder.clone(), w1.clone(), w2.clone()]);
        assert_eq!(
            escaping.get("launder"),
            Some(&HashSet::from([0])),
            "the aggregate-launder base case is escaping: {escaping:?}"
        );
        assert_eq!(
            escaping.get("w2"),
            Some(&HashSet::from([0])),
            "escaping must propagate two hops up over an aggregate-launder base: {escaping:?}"
        );
        let main = caller_passing_shared_borrow("w2");
        let errors = check_module(&[launder, w1, w2, main]);
        assert_eq!(
            count(&errors, |e| matches!(
                e,
                TypeError::LinearMoveOutOfBorrow { .. }
            )),
            1,
            "passing `&coin` into a 3-level wrapper over a launder clones the linear: {errors:?}"
        );
    }

    // A6. FIX A PRECISION: a `peek`-like fn that derefs `*r` into a LOCAL
    //     aggregate but RETURNS A SCALAR (never returns the tainted aggregate)
    //     must NOT be flagged borrow-escaping. The taint reaches the param only
    //     through `Return`; a scalar return leaves it clean.
    //     fn inspect(r:&T) -> i64 { let b = Box{item:*r}; 0 }
    #[test]
    fn fixa_deref_into_local_aggregate_returning_scalar_stays_clean() {
        let mut f = MirFunction::new("inspect", MirFnSig::new(vec![], MirType::Void));
        f.locals.push(ref_param_local(0, "r"));
        f.locals.push(linear_anon(1)); // _1 = *r
        f.locals.push(i64_local(2, "box")); // _2 = Box { item: _1 } (local, unreturned)
        let mut b0 = MirBlock::new(BlockId(0));
        b0.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Deref {
                ptr: MirValue::Local(LocalId(0)),
                pointee_ty: MirType::Struct(Arc::from("Qubit")),
            },
        ));
        b0.stmts
            .push(MirStmt::assign(LocalId(2), box_aggregate(LocalId(1))));
        // returns a SCALAR const, not the tainted aggregate.
        b0.terminator = Some(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(
            0,
            MirType::i64(),
        )))));
        f.blocks = Some(vec![b0]);

        let escaping = escaping_params_fixpoint(std::slice::from_ref(&f));
        assert!(
            escaping.get("inspect").is_none(),
            "boxing *r locally but returning a scalar is not borrow-escaping: {escaping:?}"
        );
        // And passing `&coin` into it is the legal borrow-read case.
        let main = caller_passing_shared_borrow("inspect");
        let errors = check_module(&[f, main]);
        assert!(
            errors.is_empty(),
            "a fn that boxes *r locally and returns a scalar is clean: {errors:?}"
        );
    }
}
