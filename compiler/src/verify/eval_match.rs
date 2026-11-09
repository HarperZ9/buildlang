// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Method calls and `match` in the concrete evaluator.

use super::eval::{Eval, Flow, Frame, Val, R};
use super::eval_expr::{bool_of, expr, int_of, trap, unsupported};
use super::infer::static_ty;
use super::ty::Ty;
use crate::ast::*;

pub(super) fn method_call(
    ev: &mut Eval<'_>,
    recv: &Expr,
    method: &str,
    args: &[Expr],
    fr: &mut Frame,
    expect: Option<Ty>,
) -> R<Val> {
    let env = |n: &str| fr.get(n).map(|(_, t)| t);
    let ty = static_ty(recv, &env, ev.sigs).or(expect).unwrap_or(Ty::I32);
    let x = int_of(ev.eval_value(recv, fr, Some(ty))?)?;
    let y = match args {
        [a] => Some(int_of(ev.eval_value(a, fr, Some(ty))?)?),
        [] => None,
        _ => return unsupported("method arity"),
    };
    let v = match (method, y) {
        ("wrapping_add", Some(y)) => ty.wrap(x + y),
        ("wrapping_sub", Some(y)) => ty.wrap(x - y),
        ("wrapping_mul", Some(y)) => ty.wrap(x.wrapping_mul(y)),
        ("min", Some(y)) => x.min(y),
        ("max", Some(y)) => x.max(y),
        ("abs", None) => {
            if !ty.holds(x.abs()) {
                return trap(format!("{ty} abs overflow"));
            }
            x.abs()
        }
        _ => return unsupported("method"),
    };
    Ok(Val::Int(v, ty))
}

pub(super) fn match_expr(
    ev: &mut Eval<'_>,
    scrut: &Expr,
    arms: &[MatchArm],
    fr: &mut Frame,
    expect: Option<Ty>,
) -> R<Flow> {
    let env = |n: &str| fr.get(n).map(|(_, t)| t);
    let sty = static_ty(scrut, &env, ev.sigs).unwrap_or(Ty::I32);
    let v = ev.eval_value(scrut, fr, Some(sty))?;
    for arm in arms {
        let depth = fr.depth();
        if pattern_matches(&arm.pattern, v, sty, fr)? {
            let guard_ok = match &arm.guard {
                Some(g) => bool_of(ev.eval_value(g, fr, Some(Ty::Bool))?)?,
                None => true,
            };
            if guard_ok {
                let out = expr(ev, &arm.body, fr, expect);
                fr.truncate(depth);
                return out;
            }
        }
        fr.truncate(depth);
    }
    trap("no match arm".into())
}

/// Whether `v` matches pattern `p`, binding names into `fr`.
pub(super) fn pattern_matches(p: &Pattern, v: Val, ty: Ty, fr: &mut Frame) -> R<bool> {
    Ok(match &p.kind {
        PatternKind::Wildcard => true,
        PatternKind::Ident {
            name,
            subpattern: None,
            ..
        } => {
            fr.push(&name.name, v, ty);
            true
        }
        PatternKind::Literal(Literal::Int { value, .. }) => v.int() == i128::try_from(*value).ok(),
        PatternKind::Literal(Literal::Bool(b)) => v == Val::Bool(*b),
        PatternKind::Or(ps) => {
            for q in ps {
                if pattern_matches(q, v, ty, fr)? {
                    return Ok(true);
                }
            }
            false
        }
        PatternKind::Paren(q) => pattern_matches(q, v, ty, fr)?,
        _ => return unsupported("match pattern"),
    })
}
