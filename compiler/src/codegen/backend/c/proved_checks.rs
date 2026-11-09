//! Overflow checks dropped where a proof shows they cannot fire.
//!
//! `x + 1` traps only when `x` is its type's maximum, and `x - 1` only when it
//! is the minimum. After the true edge of `x < y`, `x` is below the maximum
//! (and after `x > y`, above the minimum) until `x` is assigned again. A loop
//! counter stepped after its guard (`while i < n { ..; i = i + 1 }`) is the
//! common case.
//!
//! Two independent passes decide each site. The prover runs a forward
//! dataflow of such facts over the control-flow graph and claims the steps it
//! covers. The checker re-derives every claim with a backward search over
//! the paths that reach the step, and shares no code with the prover. Only a
//! step both accept is emitted with the wrapping helper instead of the
//! trapping one. `buildc tv` models the wrapping helper as a step that never
//! traps, so in every function it certifies, tv proves again that the source
//! step cannot trap there.
//!
//! `BUILDLANG_PLANT_PROOF=every-step` makes the prover claim every `+ 1` and
//! `- 1`. The checker must refuse each false claim, so the trap stays
//! (`compiler/tests/proved_checks.rs`). `every-step-unchecked` also skips
//! the checker, so a needed check is dropped; `buildc tv` must then refuse the
//! function.

use std::collections::{HashMap, HashSet};

use crate::codegen::ir::{
    BinOp, IntSize, LocalId, MirConst, MirFunction, MirPlace, MirRValue, MirStmtKind,
    MirTerminator, MirType, MirValue,
};

/// A statement position: (block index, statement index).
pub(super) type Site = (usize, usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Bound {
    /// The local is below its type's maximum, so `+ 1` cannot overflow.
    BelowMax,
    /// The local is above its type's minimum, so `- 1` cannot overflow.
    AboveMin,
}

/// The step at `site`, if it is `x + 1` or `x - 1` on an integer local.
fn step_at(func: &MirFunction, stmt: &MirStmtKind) -> Option<(LocalId, Bound)> {
    let MirStmtKind::Assign {
        value: MirRValue::BinaryOp { op, left, right },
        ..
    } = stmt
    else {
        return None;
    };
    let one = |v: &MirValue| {
        matches!(
            v,
            MirValue::Const(MirConst::Int(1, _)) | MirValue::Const(MirConst::Uint(1, _))
        )
    };
    let int_local = |v: &MirValue| match v {
        MirValue::Local(x)
            if matches!(
                func.locals.get(x.0 as usize).map(|l| &l.ty),
                Some(MirType::Int(..))
            ) =>
        {
            Some(*x)
        }
        _ => None,
    };
    match op {
        BinOp::Add if one(right) => int_local(left).map(|x| (x, Bound::BelowMax)),
        BinOp::Add if one(left) => int_local(right).map(|x| (x, Bound::BelowMax)),
        BinOp::Sub if one(right) => int_local(left).map(|x| (x, Bound::AboveMin)),
        _ => None,
    }
}

/// Every step site in `func`, with the decision of both passes.
pub(super) fn proved_steps(func: &MirFunction) -> HashSet<Site> {
    let Some(blocks) = func.blocks.as_ref() else {
        return HashSet::new();
    };
    let mode = std::env::var("BUILDLANG_PLANT_PROOF").unwrap_or_default();
    let plant = mode == "every-step" || mode == "every-step-unchecked";
    let claims = prover::claims(func, plant);
    if mode == "every-step-unchecked" {
        // Prover and checker both bypassed: only for the test that shows
        // `buildc tv` refuses a step whose check was dropped without a proof.
        return claims.into_iter().collect();
    }
    claims
        .into_iter()
        .filter(|&(b, s)| {
            let Some((x, bound)) = blocks
                .get(b)
                .and_then(|bl| bl.stmts.get(s))
                .and_then(|st| step_at(func, &st.kind))
            else {
                return false;
            };
            checker::holds(func, b, s, x, bound)
        })
        .collect()
}

/// Whether `bound` on `x` follows from `x op y` being true, with `y` a value
/// of `x`'s own type or an integer constant that fits it.
fn comparable(func: &MirFunction, x: LocalId, y: &MirValue) -> bool {
    let Some(MirType::Int(size, signed)) = func.locals.get(x.0 as usize).map(|l| &l.ty) else {
        return false;
    };
    match y {
        MirValue::Local(z) => {
            func.locals.get(z.0 as usize).map(|l| &l.ty) == Some(&MirType::Int(*size, *signed))
        }
        MirValue::Const(MirConst::Int(v, _)) => *signed || *v >= 0,
        MirValue::Const(MirConst::Uint(_, _)) => !matches!(size, IntSize::I128),
        _ => false,
    }
}

mod prover;

mod checker;
