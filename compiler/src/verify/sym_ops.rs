// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Operators in the symbolic executor.

use super::infer::static_ty;
use super::sym::{Out, Res, St, Sym};
use super::sym_expr::{expr, operand_ty, then, val};
use super::term::*;
use super::ty::Ty;
use crate::ast::*;

/// The term for `a op b` at integer type `ty`, adding its definedness facts.
pub fn apply(op: BinOp, ty: Ty, a: Term, b: Term, s: &mut St) -> Result<Term, String> {
    let r = match op {
        BinOp::Add => arith(AOp::Add, a, b),
        BinOp::Sub => arith(AOp::Sub, a, b),
        BinOp::Mul => arith(AOp::Mul, a, b),
        BinOp::Div | BinOp::Rem => {
            s.assume(cmp(COp::Ne, b.clone(), Term::Int(0)));
            let q = arith(AOp::Div, a.clone(), b.clone());
            let m = arith(AOp::Rem, a.clone(), b.clone());
            // Truncating division: a == q*b + m, |m| < |b|, m has a's sign.
            s.assume(cmp(
                COp::Eq,
                a.clone(),
                arith(AOp::Add, arith(AOp::Mul, q.clone(), b.clone()), m.clone()),
            ));
            if matches!(ty, Ty::Int { signed: false, .. }) {
                s.assume(cmp(COp::Ge, m.clone(), Term::Int(0)));
                s.assume(cmp(COp::Lt, m.clone(), b.clone()));
                s.assume(cmp(COp::Ge, q.clone(), Term::Int(0)));
            } else {
                let zero = || Term::Int(0);
                s.assume(or(
                    cmp(COp::Lt, a.clone(), zero()),
                    cmp(COp::Ge, m.clone(), zero()),
                ));
                s.assume(or(cmp(COp::Gt, a, zero()), cmp(COp::Le, m.clone(), zero())));
                // |m| < |b|, split on the sign of b.
                let nb = neg(b.clone());
                s.assume(or(
                    cmp(COp::Le, b.clone(), zero()),
                    cmp(COp::Lt, m.clone(), b.clone()),
                ));
                s.assume(or(
                    cmp(COp::Le, b.clone(), zero()),
                    cmp(COp::Gt, m.clone(), nb.clone()),
                ));
                s.assume(or(
                    cmp(COp::Ge, b.clone(), zero()),
                    cmp(COp::Gt, m.clone(), b.clone()),
                ));
                s.assume(or(
                    cmp(COp::Ge, b.clone(), zero()),
                    cmp(COp::Lt, m.clone(), nb),
                ));
            }
            if op == BinOp::Div {
                q
            } else {
                m
            }
        }
        BinOp::BitAnd => bits(BOp::And, ty, a, b),
        BinOp::BitOr => bits(BOp::Or, ty, a, b),
        BinOp::BitXor => bits(BOp::Xor, ty, a, b),
        BinOp::Shl => bits(BOp::Shl, ty, a, b),
        BinOp::Shr => bits(BOp::Shr, ty, a, b),
        _ => return Err("an operator outside the subset".into()),
    };
    if matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
    ) {
        s.assume(in_range(&r, ty));
    } else {
        s.fact(in_range(&r, ty));
    }
    Ok(r)
}

pub(super) fn binary(
    sym: &mut Sym<'_>,
    op: BinOp,
    l: &Expr,
    r: &Expr,
    st: St,
    expect: Option<Ty>,
) -> Res {
    if matches!(op, BinOp::And | BinOp::Or) {
        // Short-circuit: the right side runs only on the deciding paths.
        let left = expr(sym, l, st, Some(Ty::Bool))?;
        return then(sym, left, |sym, s, a, _| {
            let mut out = Vec::new();
            let decided = if op == BinOp::And {
                not(a.clone())
            } else {
                a.clone()
            };
            if decided != Term::Bool(false) {
                let mut ds = s.clone();
                ds.assume(decided);
                out.push((ds, Out::Val(Term::Bool(op == BinOp::Or), Some(Ty::Bool))));
            }
            let go_on = if op == BinOp::And {
                a.clone()
            } else {
                not(a.clone())
            };
            if go_on != Term::Bool(false) {
                let mut rs = s.clone();
                rs.assume(go_on);
                let right = expr(sym, r, rs.clone(), Some(Ty::Bool))?;
                // A right side that runs on one path and adds no facts needs
                // no split: the value is `a && b` (or `a || b`) outright.
                if let [(st, Out::Val(b, _))] = right.as_slice() {
                    if st.vars == rs.vars && st.conds.len() == rs.conds.len() {
                        let v = if op == BinOp::And {
                            and(a, b.clone())
                        } else {
                            or(a, b.clone())
                        };
                        return Ok(vec![(s, Out::Val(v, Some(Ty::Bool)))]);
                    }
                }
                out.extend(right);
            }
            Ok(out)
        });
    }
    let is_cmp = matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    );
    let shift = matches!(op, BinOp::Shl | BinOp::Shr);
    let ty = if shift {
        let env = |n: &str| st.ty_of(n);
        static_ty(l, &env, sym.sigs).or(expect).unwrap_or(Ty::I32)
    } else {
        operand_ty(sym, l, r, &st, if is_cmp { None } else { expect })
    };
    let left = expr(sym, l, st, Some(ty))?;
    then(sym, left, |sym, s, a, aty| {
        let right = expr(sym, r, s, if shift { None } else { Some(ty) })?;
        then(sym, right, |_, mut s2, b, _| {
            if is_cmp {
                let c = match op {
                    BinOp::Eq => COp::Eq,
                    BinOp::Ne => COp::Ne,
                    BinOp::Lt => COp::Lt,
                    BinOp::Le => COp::Le,
                    BinOp::Gt => COp::Gt,
                    _ => COp::Ge,
                };
                let t = if aty == Some(Ty::Bool) {
                    match c {
                        COp::Eq => iff(a.clone(), b),
                        COp::Ne => not(iff(a.clone(), b)),
                        _ => return Err("an ordering on bool".into()),
                    }
                } else {
                    cmp(c, a.clone(), b)
                };
                return val(s2, t, Some(Ty::Bool));
            }
            if aty == Some(Ty::Bool) {
                let t = match op {
                    BinOp::BitAnd => and(a.clone(), b),
                    BinOp::BitOr => or(a.clone(), b),
                    BinOp::BitXor => not(iff(a.clone(), b)),
                    _ => return Err("an operator on bool".into()),
                };
                return val(s2, t, Some(Ty::Bool));
            }
            let t = apply(op, ty, a.clone(), b, &mut s2)?;
            val(s2, t, Some(ty))
        })
    })
}
