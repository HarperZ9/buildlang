// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Method calls and `match` in the symbolic executor.

use super::infer::static_ty;
use super::sym::{Out, Res, St, Sym};
use super::sym_expr::{expr, then, val};
use super::term::*;
use super::ty::Ty;
use crate::ast::*;

pub(super) fn method_call(
    sym: &mut Sym<'_>,
    recv: &Expr,
    m: &str,
    args: &[Expr],
    st: St,
    expect: Option<Ty>,
) -> Res {
    let ty = {
        let env = |n: &str| st.ty_of(n);
        static_ty(recv, &env, sym.sigs)
            .or(expect)
            .unwrap_or(Ty::I32)
    };
    if ty.is_float() {
        let c = super::float::method_c_name(m, args.len(), ty)
            .filter(|_| sym.floats)
            .ok_or(format!("float method `{m}` outside the subset"))?;
        let r = expr(sym, recv, st, Some(ty))?;
        return then(sym, r, |sym, s, x, _| {
            let rest = match args {
                [a] => expr(sym, a, s, Some(ty))?,
                [] => return val(s, super::float::math_call(c, vec![x]), Some(ty)),
                _ => return Err("method arity".into()),
            };
            then(sym, rest, |_, s2, y, _| {
                val(s2, super::float::math_call(c, vec![x.clone(), y]), Some(ty))
            })
        });
    }
    let r = expr(sym, recv, st, Some(ty))?;
    then(sym, r, |sym, s, x, _| {
        let argv = match args {
            [a] => Some(expr(sym, a, s.clone(), Some(ty))?),
            [] => None,
            _ => return Err("method arity".into()),
        };
        let unary = |mut s: St| -> Res {
            match m {
                "abs" => {
                    let mut neg_s = s.clone();
                    neg_s.assume(cmp(COp::Lt, x.clone(), Term::Int(0)));
                    let n = neg(x.clone());
                    neg_s.assume(in_range(&n, ty));
                    s.assume(cmp(COp::Ge, x.clone(), Term::Int(0)));
                    Ok(vec![
                        (neg_s, Out::Val(n, Some(ty))),
                        (s, Out::Val(x.clone(), Some(ty))),
                    ])
                }
                _ => Err(format!("method `{m}`")),
            }
        };
        let Some(argv) = argv else { return unary(s) };
        then(sym, argv, |_, s2, y, _| match m {
            "wrapping_add" => val(s2, bits(BOp::WAdd, ty, x.clone(), y), Some(ty)),
            "wrapping_mul" => val(s2, bits(BOp::WMul, ty, x.clone(), y), Some(ty)),
            "wrapping_sub" => {
                let t = Term::App(format!("wrapping_sub_{ty}"), vec![x.clone(), y]);
                let mut s3 = s2;
                s3.fact(in_range(&t, ty));
                val(s3, t, Some(ty))
            }
            "min" | "max" => {
                let first = cmp(
                    if m == "min" { COp::Le } else { COp::Ge },
                    x.clone(),
                    y.clone(),
                );
                let mut a = s2.clone();
                a.assume(first.clone());
                let mut b = s2;
                b.assume(not(first));
                Ok(vec![
                    (a, Out::Val(x.clone(), Some(ty))),
                    (b, Out::Val(y, Some(ty))),
                ])
            }
            _ => Err(format!("method `{m}`")),
        })
    })
}

pub(super) fn match_arms(
    sym: &mut Sym<'_>,
    arms: &[MatchArm],
    i: usize,
    st: St,
    v: &Term,
    ty: Ty,
    expect: Option<Ty>,
) -> Res {
    let Some(arm) = arms.get(i) else {
        // No arm matched: the program traps, so no finishing path continues.
        return Ok(Vec::new());
    };
    let mut binds = Vec::new();
    let hit = pattern_cond(&arm.pattern, v, ty, &mut binds)?;
    let mut out = Vec::new();
    if hit != Term::Bool(false) {
        let mut s = st.clone();
        s.assume(hit.clone());
        let depth = s.vars.len();
        for (n, t) in &binds {
            s.vars.push((n.clone(), t.clone(), ty));
        }
        let guarded = match &arm.guard {
            None => vec![(s, Out::Val(Term::Bool(true), Some(Ty::Bool)))],
            Some(g) => expr(sym, g, s, Some(Ty::Bool))?,
        };
        for (gs, o) in guarded {
            let Out::Val(g, _) = o else { continue };
            if g != Term::Bool(false) {
                let mut body_s = gs.clone();
                body_s.assume(g.clone());
                for (mut bs, bo) in expr(sym, &arm.body, body_s, expect)? {
                    bs.vars.truncate(depth);
                    out.push((bs, bo));
                }
            }
            if g != Term::Bool(true) {
                let mut miss = gs;
                miss.vars.truncate(depth);
                miss.assume(not(g));
                out.extend(match_arms(sym, arms, i + 1, miss, v, ty, expect)?);
            }
        }
    }
    if hit != Term::Bool(true) {
        let mut miss = st;
        miss.assume(not(hit));
        out.extend(match_arms(sym, arms, i + 1, miss, v, ty, expect)?);
    }
    Ok(out)
}

fn pattern_cond(
    p: &Pattern,
    v: &Term,
    ty: Ty,
    binds: &mut Vec<(String, Term)>,
) -> Result<Term, String> {
    Ok(match &p.kind {
        PatternKind::Wildcard => Term::Bool(true),
        PatternKind::Ident {
            name,
            subpattern: None,
            ..
        } => {
            binds.push((name.name.to_string(), v.clone()));
            Term::Bool(true)
        }
        PatternKind::Literal(Literal::Int { value, .. }) => {
            let n = i128::try_from(*value).map_err(|_| "a literal pattern".to_string())?;
            cmp(COp::Eq, v.clone(), Term::Int(n))
        }
        PatternKind::Literal(Literal::Bool(b)) => iff(v.clone(), Term::Bool(*b)),
        PatternKind::Or(ps) => {
            let mut acc = Term::Bool(false);
            for q in ps {
                acc = or(acc, pattern_cond(q, v, ty, binds)?);
            }
            acc
        }
        PatternKind::Paren(q) => pattern_cond(q, v, ty, binds)?,
        _ => return Err("a match pattern outside the subset".into()),
    })
}
