// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Expression cases of the symbolic executor.

use super::float;
use super::infer::{assoc_const, callee_name, is_float_lit, static_ty, var_name};
use super::sym::{Out, Res, St, Sym};
use super::sym_match::{match_arms, method_call};
use super::sym_ops::{apply, binary};
use super::term::*;
use super::ty::Ty;
use crate::ast::*;

pub(super) fn val(st: St, t: Term, ty: Option<Ty>) -> Res {
    Ok(vec![(st, Out::Val(t, ty))])
}

/// Run `f` on the value of each path of `res`; returns pass through.
pub(super) fn then(
    sym: &mut Sym<'_>,
    res: Vec<(St, Out)>,
    mut f: impl FnMut(&mut Sym<'_>, St, Term, Option<Ty>) -> Res,
) -> Res {
    let mut out = Vec::new();
    for (s, o) in res {
        match o {
            Out::Val(t, ty) => out.extend(f(sym, s, t, ty)?),
            r => out.push((s, r)),
        }
        if out.len() > sym.max_paths {
            return Err("too many paths".into());
        }
    }
    Ok(out)
}

pub(super) fn operand_ty(sym: &Sym<'_>, l: &Expr, r: &Expr, st: &St, expect: Option<Ty>) -> Ty {
    let env = |n: &str| sym.env_ty(st, n);
    static_ty(l, &env, sym.sigs)
        .or_else(|| static_ty(r, &env, sym.sigs))
        .or(expect)
        .unwrap_or(if sym.floats && (is_float_lit(l) || is_float_lit(r)) {
            Ty::F64
        } else {
            Ty::I32
        })
}

pub(super) fn expr(sym: &mut Sym<'_>, e: &Expr, st: St, expect: Option<Ty>) -> Res {
    match &e.kind {
        ExprKind::Literal(Literal::Int { value, suffix, .. }) => {
            let ty = suffix
                .and_then(Ty::from_suffix)
                .or(expect.filter(|t| t.is_int()))
                .unwrap_or(Ty::I32);
            let v = i128::try_from(*value).map_err(|_| "a literal".to_string())?;
            val(st, Term::Int(v), Some(ty))
        }
        ExprKind::Literal(Literal::Bool(b)) => val(st, Term::Bool(*b), Some(Ty::Bool)),
        ExprKind::Literal(Literal::Float { value, suffix }) => {
            if !sym.floats {
                return Err("a float literal (outside the subset)".into());
            }
            let ty = match suffix {
                Some(FloatSuffix::F32) => Ty::F32,
                Some(FloatSuffix::F64) => Ty::F64,
                Some(FloatSuffix::F16) => return Err("an f16 literal".into()),
                None => expect.filter(|t| t.is_float()).unwrap_or(Ty::F64),
            };
            val(st, float::fconst(ty, *value), Some(ty))
        }
        ExprKind::Paren(inner) => expr(sym, inner, st, expect),
        ExprKind::Ident(_) | ExprKind::Path(_) => {
            if let Some((ty, v)) = assoc_const(e) {
                return val(st, Term::Int(v), Some(ty));
            }
            let n = var_name(e).ok_or("a path")?;
            let (t, ty) = st
                .get(&n)
                .or_else(|| sym.consts.get(&n).cloned())
                .ok_or(format!("unknown name `{n}`"))?;
            val(st, t, Some(ty))
        }
        ExprKind::Unary { op, expr: inner } => {
            let op = *op;
            let r = expr(sym, inner, st, expect)?;
            then(sym, r, move |_, mut s, t, ty| match (op, ty) {
                (UnaryOp::Not, Some(Ty::Bool)) => val(s, not(t), ty),
                (UnaryOp::Not | UnaryOp::BitNot, Some(it @ Ty::Int { .. })) => {
                    let r = Term::BitNot(it, Box::new(t));
                    s.fact(in_range(&r, it));
                    val(s, r, ty)
                }
                (UnaryOp::Neg, Some(ft @ Ty::Float { .. })) => {
                    val(s, float::fop("neg", ft, vec![t]), ty)
                }
                (UnaryOp::Neg, Some(it @ Ty::Int { .. })) => {
                    let r = neg(t);
                    s.assume(in_range(&r, it));
                    val(s, r, ty)
                }
                _ => Err("a unary operator outside the subset".into()),
            })
        }
        ExprKind::Binary { op, left, right } => binary(sym, *op, left, right, st, expect),
        ExprKind::Assign { op, target, value } if matches!(target.kind, ExprKind::Field { .. }) => {
            super::sym_agg::assign_field(sym, *op, target, value, st)
        }
        ExprKind::Struct { path, fields, rest } => {
            super::sym_agg::literal(sym, path, fields, rest.as_deref(), st)
        }
        ExprKind::Field { expr: base, field } => super::sym_agg::field(sym, base, field, st),
        ExprKind::Assign { op, target, value } => {
            let n = var_name(target).ok_or("an assignment target")?;
            let ty = st.ty_of(&n).ok_or("an assignment target")?;
            let bop = op.to_bin_op();
            if ty.is_float() && bop.is_some() {
                return Err("a compound float assignment".into());
            }
            let r = expr(sym, value, st, Some(ty))?;
            then(sym, r, |_, mut s, t, _| {
                let nv = match bop {
                    None => t,
                    Some(b) => {
                        let cur = s.get(&n).unwrap().0;
                        apply(b, ty, cur, t, &mut s)?
                    }
                };
                s.set(&n, nv);
                val(s, Term::Int(0), None)
            })
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let c = expr(sym, condition, st, Some(Ty::Bool))?;
            then(sym, c, |sym, s, cond, _| {
                if let Some(merged) =
                    merge_bool_if(sym, &s, &cond, then_branch, else_branch.as_deref(), expect)?
                {
                    return Ok(merged);
                }
                let mut out = Vec::new();
                if cond != Term::Bool(false) {
                    let mut ts = s.clone();
                    ts.assume(cond.clone());
                    out.extend(sym.block(then_branch, ts, expect)?);
                }
                if cond != Term::Bool(true) {
                    let mut es = s;
                    es.assume(not(cond));
                    match else_branch {
                        Some(el) => out.extend(expr(sym, el, es, expect)?),
                        None => out.push((es, Out::Val(Term::Int(0), None))),
                    }
                }
                Ok(out)
            })
        }
        ExprKind::Block(b) => sym.block(b, st, expect),
        ExprKind::Match { scrutinee, arms } => {
            let sty = {
                let env = |n: &str| sym.env_ty(&st, n);
                static_ty(scrutinee, &env, sym.sigs).unwrap_or(Ty::I32)
            };
            if sty.is_float() {
                return Err("a match on a float".into());
            }
            let r = expr(sym, scrutinee, st, Some(sty))?;
            then(sym, r, |sym, s, v, _| {
                match_arms(sym, arms, 0, s, &v, sty, expect)
            })
        }
        ExprKind::Return(v) => {
            let ret = st.ret;
            match v {
                None => Ok(vec![(st, Out::Ret(Term::Int(0), None))]),
                Some(v) => {
                    let r = expr(sym, v, st, ret)?;
                    then(sym, r, |_, s, t, ty| Ok(vec![(s, Out::Ret(t, ty))]))
                }
            }
        }
        ExprKind::Cast { expr: inner, ty } => {
            let to = sym.ty_of_ast(ty).ok_or("a cast type outside the subset")?;
            let r = expr(sym, inner, st, None)?;
            then(sym, r, move |_, mut s, t, from| {
                if to.is_float() || from.is_some_and(|f| f.is_float()) {
                    let t = float::conv(to, from, t)?;
                    s.fact(in_range(&t, to));
                    return val(s, t, Some(to));
                }
                let t = match from {
                    Some(Ty::Bool) => t,
                    _ => cast(to, from, t),
                };
                s.fact(in_range(&t, to));
                val(s, t, Some(to))
            })
        }
        ExprKind::Call { func, args } => {
            let name = callee_name(func).ok_or("a callee that is not a name")?;
            if !sym.fns.contains_key(&name) {
                if let (true, Some(c)) = (sym.floats, float::builtin_c_name(&name, args.len())) {
                    let ptys = vec![Some(Ty::F64); args.len()];
                    return eval_args(sym, args, &ptys, 0, st, Vec::new(), &mut |_, s, terms| {
                        val(s, float::math_call(c, terms), Some(Ty::F64))
                    });
                }
                return Err(format!("call to `{name}` outside the subset"));
            }
            let ptys = sym
                .sigs
                .fns
                .get(&name)
                .map(|s| s.0.clone())
                .unwrap_or_default();
            eval_args(sym, args, &ptys, 0, st, Vec::new(), &mut |sym, s, terms| {
                sym.user_call(&name, terms, s)
            })
        }
        ExprKind::MethodCall {
            receiver,
            method,
            args,
            ..
        } => method_call(sym, receiver, &method.name, args, st, expect),
        ExprKind::Macro { path, .. } if sym.source.is_some() => {
            super::sym_effects::print_macro(sym, e, path, st)
        }
        ExprKind::Break {
            value: None,
            label: None,
        } => Ok(vec![(st, Out::Break)]),
        ExprKind::Continue { label: None } => Ok(vec![(st, Out::Continue)]),
        ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::For { .. } => {
            Err("a loop (outside the symbolic subset)".into())
        }
        _ => Err("an expression kind outside the subset".into()),
    }
}

/// An `if` whose branches each finish on one path, add no facts, assign
/// nothing and yield a bool becomes one path with the value
/// `(c && then) || (!c && else)`, instead of two paths.
fn merge_bool_if(
    sym: &mut Sym<'_>,
    s: &St,
    cond: &Term,
    then_branch: &Block,
    else_branch: Option<&Expr>,
    expect: Option<Ty>,
) -> Result<Option<Vec<(St, Out)>>, String> {
    if matches!(cond, Term::Bool(_)) {
        return Ok(None);
    }
    probe(sym, s, cond, then_branch, else_branch, expect)
}

fn probe(
    sym: &mut Sym<'_>,
    s: &St,
    cond: &Term,
    then_branch: &Block,
    else_branch: Option<&Expr>,
    expect: Option<Ty>,
) -> Result<Option<Vec<(St, Out)>>, String> {
    let Some(el) = else_branch else {
        return Ok(None);
    };
    let mut ts = s.clone();
    ts.assume(cond.clone());
    let mut es = s.clone();
    es.assume(not(cond.clone()));
    let tr = sym.block(then_branch, ts.clone(), expect)?;
    let er = expr(sym, el, es.clone(), expect)?;
    let single = |r: &[(St, Out)], start: &St| -> Option<Term> {
        match r {
            [(st, Out::Val(v, Some(Ty::Bool)))]
                if st.vars == start.vars
                    && st.conds.len() == start.conds.len()
                    && st.trace.len() == start.trace.len() =>
            {
                Some(v.clone())
            }
            _ => None,
        }
    };
    match (single(&tr, &ts), single(&er, &es)) {
        (Some(v), Some(w)) => {
            let merged = or(and(cond.clone(), v), and(not(cond.clone()), w));
            Ok(Some(vec![(s.clone(), Out::Val(merged, Some(Ty::Bool)))]))
        }
        _ => {
            let mut out = tr;
            out.extend(er);
            Ok(Some(out))
        }
    }
}

type ArgK<'k> = dyn FnMut(&mut Sym<'_>, St, Vec<Term>) -> Res + 'k;

fn eval_args(
    sym: &mut Sym<'_>,
    args: &[Expr],
    ptys: &[Option<Ty>],
    i: usize,
    st: St,
    acc: Vec<Term>,
    k: &mut ArgK<'_>,
) -> Res {
    if i == args.len() {
        return k(sym, st, acc);
    }
    let want = ptys.get(i).copied().flatten();
    let r = expr(sym, &args[i], st, want)?;
    then(sym, r, |sym, s, t, _| {
        let mut a = acc.clone();
        a.push(t);
        eval_args(sym, args, ptys, i + 1, s, a, k)
    })
}
